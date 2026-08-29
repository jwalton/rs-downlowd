use std::{
    env,
    path::PathBuf,
    sync::{LazyLock, OnceLock},
    thread,
    time::Duration,
};

use testcontainers::{
    Container, ContainerAsync, ContainerRequest, GenericImage, ImageExt,
    core::{AccessMode, ContainerPort, Mount},
};
use tokio::sync::Mutex;

fn get_project_root() -> PathBuf {
    env::current_dir().unwrap()
}

static SYNC_WEB_SERVER: OnceLock<(Container<GenericImage>, String)> = OnceLock::new();

type AsyncWebServerMutex = Mutex<Option<(ContainerAsync<GenericImage>, String)>>;
static ASYNC_WEB_SERVER: LazyLock<AsyncWebServerMutex> = LazyLock::new(|| Mutex::new(None));

fn configure_web_server() -> ContainerRequest<GenericImage> {
    let project_root = get_project_root();
    let public_dir = project_root.join("test-support/static");

    GenericImage::new("nginx", "1-alpine")
        .with_exposed_port(ContainerPort::Tcp(80))
        .with_mount(
            Mount::bind_mount(public_dir.to_str().unwrap(), "/usr/share/nginx/html")
                .with_access_mode(AccessMode::ReadOnly),
        )
    // Need static-web-server 3.x to use `--etag` option.
    // .with_cmd(["--etag"])
}

/// Start a web server container and return the URL used to access it.
///
pub fn spawn_web_server() -> &'static str {
    use testcontainers::runners::SyncRunner;

    let (_, url) = SYNC_WEB_SERVER.get_or_init(|| {
        let container = configure_web_server().start().unwrap();

        let host = container.get_host().unwrap();
        let port = container
            .get_host_port_ipv4(ContainerPort::Tcp(80))
            .unwrap();

        let url = format!("http://{host}:{port}");

        // Give the server a moment to start up.
        thread::sleep(Duration::from_millis(500));

        (container, url)
    });

    url
}

/// Start a web server container and return the URL used to access it.
///
pub async fn spawn_web_server_async() -> String {
    use testcontainers::runners::AsyncRunner;

    let mut server = ASYNC_WEB_SERVER.lock().await;

    match server.as_mut() {
        None => {
            let container = configure_web_server().start().await.unwrap();

            let host = container.get_host().await.unwrap();
            let port = container
                .get_host_port_ipv4(ContainerPort::Tcp(80))
                .await
                .unwrap();

            let url = format!("http://{host}:{port}");

            // Give the server a moment to start up.
            tokio::time::sleep(Duration::from_millis(500)).await;

            *server = Some((container, url.clone()));
            url.clone()
        }
        Some((_, url)) => url.clone(),
    }
}
