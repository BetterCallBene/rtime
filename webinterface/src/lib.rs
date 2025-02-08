use actix_web::{get, web, App, HttpServer, Responder};
use std::os::raw::{c_char, c_int};
use std::sync::Mutex;
use tokio::runtime::Runtime;

use log::{debug, error, info, warn};

static SUMMARY_MESSAGE: &str = "{
    \"name\": \"webinterface\",
    \"summary\": \"web backend\",
    \"library_type\": \"Service\",
    \"version\": \"0.1.0\",
    \"provides\": [
        {
            \"capability\": \"webinterface_start\",
            \"entry\": \"start\"
        },
        {
            \"capability\": \"webinterface_stop\",
            \"entry\": \"stop\"
        }
    ],
    \"requires\": [\"blackboard\"]
}\0";

struct Config {
    hostname: String,
    port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hostname: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl Config {
    fn new(key_values: &Vec<interfaces::blackboard::BlackboardEntry>) -> Self {
        let mut config = Self::default();
        for entry in key_values {
            match entry.key.as_str() {
                "hostname" => {
                    if let interfaces::blackboard::BlackboardValue::String(value) = &entry.value {
                        config.hostname = value.clone();
                    }
                }
                "port" => {
                    if let interfaces::blackboard::BlackboardValue::Int(value) = &entry.value {
                        config.port = value.clone() as u16;
                    }
                }
                _ => {}
            }
        }
        config
    }
}

#[get("/startproject")]
async fn start_project(data: web::Data<AppData>) -> impl Responder {
    data.caps
        .get("blackboard_set_string")
        .map(|cap| {
            unsafe {
                let f: interfaces::capabilities::Function<
                    unsafe extern "C" fn(*const c_char, *const c_char) -> c_int,
                > = cap.get().unwrap();
                let result = f(
                    "start_project\0".as_ptr() as *const c_char,
                    "{\"value\": \"Hello World\"}\0".as_ptr() as *const c_char,
                );

                debug!("Start server project: {}", result);
                format!("Start project: {}", result)
            }
        })
        .unwrap_or_else(|| "Capability not found".to_string());
    // let state = SERVER_STATE.lock().unwrap();
    // if let Some(server_state) = &*state {
    //     let caps = &server_state.caps;
    //     let cap = caps.get("blackboard_set_string").unwrap();
    //     unsafe {
    //         let f: interfaces::capabilities::Function<unsafe extern "C" fn(*const c_char, *const c_char) -> c_int > = cap.get().unwrap();
    //         let result = f("start_project\0".as_ptr() as *const c_char, "{\"value\": \"Hello World\"}\0".as_ptr() as *const c_char);

    //         debug!("Start server project: {}", result);
    //         return format!("Start project: {}", result);
    //     }
    // }

    format!("Hello world!")
}

fn config_app(cfg: &mut web::ServiceConfig) {
    cfg.service(start_project);
}

// Shared state to hold the server handle and shutdown signal
struct ServerState {
    server_task: tokio::task::JoinHandle<()>,
    server_handle: actix_web::dev::ServerHandle,
    rt: Runtime,
}

struct AppData {
    caps: interfaces::capabilities::Capabilities,
}

lazy_static::lazy_static! {
    static ref SERVER_STATE: Mutex<Option<ServerState>> = Mutex::new(None);
}

#[no_mangle]
pub extern "C" fn summary() -> *const c_char {
    // summary message + null terminator
    SUMMARY_MESSAGE.as_ptr() as *const c_char
}

fn start_server(
    caps: &interfaces::bindings::Capabilities,
    attributes: *const c_char,
) -> Result<(), String> {
    let mut state = SERVER_STATE.lock().unwrap();
    if state.is_some() {
        return Err("Server is already running.".to_string());
    }
    let config = |attr: *const c_char| -> Result<*const c_char, String> {
        if !attr.is_null() {
            Ok(attr)
        } else {
            Err("Incoming attributes are null ".to_string())
        }
    }(attributes)
    .and_then(|att: *const c_char| -> Result<&str, String> {
        unsafe {
            std::ffi::CStr::from_ptr(att)
                .to_str()
                .map_err(|e| format!("Cannot convert incming attributes to string: {}", e))
        }
    })
    .and_then(
        |att: &str| -> Result<Vec<interfaces::blackboard::BlackboardEntry>, String> {
            serde_yml::from_str(att).map_err(|e| e.to_string())
        },
    )
    .map_err(|e: String| -> () { warn!("Error parsing attributes: {:?}", e) })
    .map(|att: Vec<interfaces::blackboard::BlackboardEntry>| -> Config { Config::new(&att) })
    .unwrap_or_default();

    info!("Starting server....");

    let data = web::Data::new(AppData {
        caps: interfaces::capabilities::Capabilities::from_raw(caps),
    });

    let rt = Runtime::new().map_err(|e| format!("Error starting async runtime\n Reason: {}", e))?;
    let bind_server = HttpServer::new( move || App::new().configure(config_app)
        .app_data(data.clone())
)
        .bind((config.hostname, config.port as u16))
        .map_err(|e| format!("Error binding server\n Reason: {}", e))?;
    let server = bind_server.run();
    let server_handle: actix_web::dev::ServerHandle = server.handle();

    let server_task = rt.spawn(async move {
        server.await.unwrap();
    });

    let server_state = ServerState {
        server_task: server_task,
        server_handle: server_handle,
        rt,
    };

    {
        *state = Some(server_state);
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn start(
    caps: &interfaces::bindings::Capabilities,
    attributes: *const c_char,
) -> i32 {
    env_logger::init();
    match start_server(caps, attributes) {
        Ok(_) => {
            info!("Server started");
            0
        }
        Err(e) => {
            error!("Error starting server: {:?}", e);
            -1
        }
    }
}

fn stop_server() -> Result<(), String> {
    let mut state = SERVER_STATE
        .lock()
        .map_err(|e| format!("Error locking server state: {:?}", e))?;

    if state.is_none() {
        return Err("Server is not running".to_string());
    }

    info!("Stopping server");
    let server_state = state.take().unwrap();
    let rt = server_state.rt;

    rt.spawn(async move {
        server_state.server_handle.stop(true).await;
        debug!("Send stop signal to server");
    });

    rt.block_on(server_state.server_task)
        .map_err(|e| format!("Error stopping server: {:?}", e))?;

    *state = None;

    Ok(())
}

#[no_mangle]
pub extern "C" fn stop() -> i32 {
    match stop_server() {
        Ok(_) => {
            info!("Server stopped");
            0
        }
        Err(e) => {
            error!("Error stopping server: {:?}", e);
            -1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libc::strlen;
    use log::trace;
    use rstest::fixture;
    use rstest::rstest;
    use serial_test::serial;
    use std::os::raw::c_int;

    #[test]
    #[serial]
    fn test_summary() {
        let result = &String::from(SUMMARY_MESSAGE)[0..SUMMARY_MESSAGE.len() - 1]; // remove null terminator
        let summary_result_c = summary();
        let summary_result = unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                summary_result_c as *const u8,
                strlen(summary_result_c),
            ))
        };

        assert_eq!(result, summary_result);
    }

    #[test_log::test]
    #[serial]
    fn test_startup() {
        let caps = interfaces::capabilities::Capabilities::new();

        let result = start_server(caps.inner(), std::ptr::null());
        assert_eq!(result.is_ok(), true);

        let result = start_server(caps.inner(), std::ptr::null());
        assert_eq!(result.is_err(), true);

        // sleep for 1 second to allow server to start
        info!("Sleeping for 1 seconds");
        std::thread::sleep(std::time::Duration::from_secs(1));

        let _ = stop();

        let result = stop();
        assert_eq!(result, -1);

        let config = vec![
            interfaces::blackboard::BlackboardEntry {
                key: "hostname".to_string(),
                value: interfaces::blackboard::BlackboardValue::String("127.0.0.1".to_string()),
            },
            interfaces::blackboard::BlackboardEntry {
                key: "port".to_string(),
                value: interfaces::blackboard::BlackboardValue::Int(3333),
            },
        ]; // empty config

        let config = serde_yml::to_string(&config).unwrap() + "\0";
        let result = start_server(caps.inner(), config.as_ptr() as *const c_char);

        assert_eq!(result.is_ok(), true);

        info!("Sleeping for 1 seconds");
        std::thread::sleep(std::time::Duration::from_secs(1));

        let result = stop();
        assert_eq!(result, 0);
    }

    #[fixture]
    fn startup() -> c_int {
        let _result = stop();
        let caps = interfaces::capabilities::Capabilities::new();
        let result = start_server(caps.inner(), std::ptr::null());
        return if result.is_ok() { 0 } else { -1 };
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_greet(startup: c_int) {
        assert_eq!(startup, 0);

        let fetch_page = async {
            let client = reqwest::Client::new();
            let res = client
                .get("http://localhost:8080/hello/world")
                .send()
                .await
                .unwrap();
            let body = res.text().await.unwrap();
            assert_eq!(body, "Hello world!");
        };

        {
            trace!("Spawning fetch_page");
            let state = SERVER_STATE.lock().unwrap();
            let rt = &state.as_ref().unwrap().rt;
            rt.spawn(fetch_page);
        }
        trace!("Sleeping for 1 seconds");
        std::thread::sleep(std::time::Duration::from_secs(3));
        let result: i32 = stop();
        assert_eq!(result, 0);
    }
}
