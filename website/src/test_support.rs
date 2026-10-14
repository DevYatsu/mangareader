//! In-process HTTP harness for route tests.
//!
//! Drives the real router (`crate::app`) over an in-memory SQLite database.
//! No port is bound and the sync loop never starts.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

use crate::AppState;

pub struct TestApp {
    pub router: Router,
    pub state: Arc<AppState>,
    /// The setup token armed at boot, exactly as `main` would.
    pub setup_token: Option<String>,
    /// Every email the app "sent".
    pub mailer: Arc<crate::email::MemoryMailer>,
    /// Every background job the app asked to start (none actually run).
    pub jobs: Arc<std::sync::Mutex<Vec<crate::scraper_helper::Job>>>,
}

/// A fresh app over an empty, initialized in-memory database, booted like
/// production: with zero users, setup is pending.
pub async fn test_app() -> TestApp {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    mangaread_core::db::init_db(&conn).expect("init db");
    let mailer = Arc::new(crate::email::MemoryMailer::default());
    let jobs = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = jobs.clone();
    let state = Arc::new(AppState {
        mailer: mailer.clone(),
        spawn_job: Arc::new(move |job| {
            recorder.lock().unwrap().push(job);
            true
        }),
        ..AppState::new(conn)
    });
    let setup_token = crate::routes::setup::arm_if_needed(&state).await;
    TestApp {
        router: crate::app(state.clone()),
        state,
        setup_token,
        mailer,
        jobs,
    }
}

/// A fresh app with setup completed. Returns the admin's session cookie.
pub async fn app_with_admin() -> (TestApp, String) {
    let app = test_app().await;
    let token = app.setup_token.clone().expect("fresh app is armed");
    let res = app
        .post_form(
            "/setup",
            &[
                ("token", &token),
                ("name", "Owner"),
                ("email", "owner@example.com"),
                ("password", "correct horse"),
            ],
            None,
        )
        .await;
    let cookie = res.session.expect("setup logs the admin in");
    (app, cookie)
}

/// A captured response: status, `Location` and `Set-Cookie` headers, body text.
pub struct Res {
    pub status: StatusCode,
    pub location: Option<String>,
    /// `name=value` of the session cookie when the response sets one.
    pub session: Option<String>,
    pub body: String,
}

impl TestApp {
    /// Store a site setting as the admin would.
    pub async fn set(&self, key: &str, value: &str) {
        let db = self.state.db.lock().await;
        mangaread_core::db::set_setting(&db, key, value, 1).unwrap();
    }

    /// Register a member directly in the DB and return a session cookie.
    pub async fn member(&self, email: &str) -> (i64, String) {
        let db = self.state.db.lock().await;
        let id = mangaread_core::db::create_user(
            &db,
            &mangaread_core::domain::NewUser {
                name: email.split('@').next().unwrap().into(),
                email: email.into(),
                password_hash: crate::auth::hash_password("password1").unwrap(),
                role: mangaread_core::domain::Role::Member,
            },
        )
        .unwrap();
        let token = crate::auth::new_token();
        mangaread_core::db::insert_session(&db, id, &token, "2099-01-01 00:00:00", "test").unwrap();
        (id, format!("{}={token}", crate::auth::SESSION_COOKIE))
    }

    async fn send(&self, req: Request<Body>) -> Res {
        let res = self.router.clone().oneshot(req).await.expect("infallible");
        let status = res.status();
        let location = res
            .headers()
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_string());
        let session = res
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|c| c.starts_with(crate::auth::SESSION_COOKIE) && !c.contains("Max-Age=0"))
            .map(|c| c.split(';').next().unwrap().to_string());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        Res {
            status,
            location,
            session,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        }
    }

    /// GET as a browser (`Accept: text/html`), optionally with a session cookie.
    pub async fn get(&self, path: &str, cookie: Option<&str>) -> Res {
        let mut req = Request::get(path).header(header::ACCEPT, "text/html");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        self.send(req.body(Body::empty()).unwrap()).await
    }

    /// POST an urlencoded form, optionally with a session cookie.
    pub async fn post_form(&self, path: &str, form: &[(&str, &str)], cookie: Option<&str>) -> Res {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        let mut req = Request::post(path)
            .header(header::ACCEPT, "text/html")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        self.send(req.body(Body::from(body)).unwrap()).await
    }
}
