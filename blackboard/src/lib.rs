use interfaces::blackboard::{BlackboardEntry, BlackboardValue};
use log::{debug, error, info, trace};
use once_cell::sync::OnceCell;
use std::any::Any;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Mutex;
use std::vec::Vec;

static SUMMARY_MESSAGE: &str = "{
    \"name\": \"blackboard\",
    \"version\": \"0.1.0\",
    \"library_type\": \"Service\",
    \"provides\": [
        {
            \"capability\": \"blackboard_start\",
            \"entry\": \"start\"
        },
        {
            \"capability\": \"blackboard_stop\",
            \"entry\": \"stop\"
        },
        {
            \"capability\": \"blackboard_reset\",
            \"entry\": \"reset\"
        },
        {
            \"capability\": \"blackboard_size\",
            \"entry\": \"size\"
        },
        {
            \"capability\": \"blackboard_get_string\",
            \"entry\": \"get_string\"
        },
        {
            \"capability\": \"blackboard_set_string\",
            \"entry\": \"set_string\"
        },
        {
            \"capability\": \"blackboard_get_int\",
            \"entry\": \"get_int\"
        },
        {
            \"capability\": \"blackboard_set_int\",
            \"entry\": \"set_int\"
        },
        {
            \"capability\": \"blackboard_get_bool\",
            \"entry\": \"get_bool\"
        },
        {
            \"capability\": \"blackboard_set_bool\",
            \"entry\": \"set_bool\"
        },
        {
            \"capability\": \"blackboard_get_float\",
            \"entry\": \"get_float\"
        },
        {
            \"capability\": \"blackboard_set_float\",
            \"entry\": \"set_float\"
        },
        {
            \"capability\": \"blackboard_get_double\",
            \"entry\": \"get_double\"
        },
        {
            \"capability\": \"blackboard_set_double\",
            \"entry\": \"set_double\"
        },
        {
            \"capability\": \"blackboard_as_json_schema\",
            \"entry\": \"as_json_schema\"
        },
        {
            \"capability\": \"blackboard_subscribe\",
            \"entry\": \"subscribe\"
        },
        { 
            \"capability\": \"blackboard_unsubscribe\",
            \"entry\": \"unsubscribe\"
        }
    ]
}\0";

#[derive(Debug)]
struct BlackBoardData {
    data: HashMap<String, Box<dyn Any + Send>>,
    listener: interfaces::capabilities::Capabilities,
    user_data: HashMap<String, *mut c_void>,
    key_to_listener: HashMap<String, Vec<String>>, // blackboard key
}

unsafe impl Send for BlackBoardData {}
unsafe impl Sync for BlackBoardData {}

impl BlackBoardData {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
            listener: interfaces::capabilities::Capabilities::new(),
            user_data: HashMap::new(),
            key_to_listener: HashMap::new(),
        }
    }

    fn subscribe(&mut self, key: &str, component: &str, callback: *mut c_void, user_data: *mut c_void) {
        let listener_key = format!("{}_{}", key, component);

        if callback.is_null() {
            error!("Provided callback is null");
            return;
        }

        if !self.key_to_listener.contains_key(key) {
            self.key_to_listener
                .insert(key.to_string(), vec![listener_key.clone()]);
        } else {
            if self
                .key_to_listener
                .get_mut(key)
                .unwrap()
                .contains(&listener_key)
            {
                debug!("Already subscribed");
                return;
            }
            self.key_to_listener
                .get_mut(key)
                .unwrap()
                .push(listener_key.clone());
        }

        let cap = interfaces::capabilities::Capability::new(&listener_key, callback);
        self.listener.add(cap);

        if !user_data.is_null() {
            self.user_data.insert(listener_key, user_data);
        }

        debug!("Subscribing to key: {}", key);
    }

    fn unsubscribe(&mut self, key: &str, component: &str) {
        let listener_key = format!("{}_{}", key, component);

        if !self.key_to_listener.contains_key(key) {
            debug!("No subscribers for key: {}", key);
            return;
        }

        let listeners = self.key_to_listener.get_mut(key).unwrap();
        listeners.retain(|x| x != &listener_key);

        // we need to remove the capability, too. but we do it later

        if self.key_to_listener.get(key).unwrap().len() == 0 {
            self.key_to_listener.remove(key);
        }

        if self.user_data.contains_key(&listener_key) {
            self.user_data.remove(&listener_key);
        }

        info!("Unsubscribing from key: {}", key);
    }

    fn notify(&self, key: &str) {
        if !self.key_to_listener.contains_key(key) {
            debug!("No subscribers for key: {}", key);
            return;
        }

        trace!("Notifying subscribers for key: {}", key);
        let listeners = self.key_to_listener.get(key).unwrap();

        for listener in listeners {
            trace!("Notifying listener: {}", listener);
            let cap = self.listener.get(listener).unwrap();
            
            unsafe {
                let f: interfaces::capabilities::Function<
                    unsafe extern "C" fn(key: *const c_char, user_data: *mut c_void) -> c_int,
                > = cap.get().unwrap();
                trace!("Calling listener: {}", listener);
                if self.user_data.contains_key(listener) && !self.user_data.get(listener).unwrap().is_null() {
                    let user_data = self.user_data.get(listener).unwrap().clone();
                    f(key.as_ptr() as *const c_char, user_data);
                } else {
                    f(key.as_ptr() as *const c_char, std::ptr::null_mut());
                }
                trace!("Listener called: {}", listener);
            }
        }
    }

    fn is_key_valid(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    fn set<T: 'static + std::marker::Send>(&mut self, key: &str, value: T) {
        if !self.data.contains_key(key) {
            self.data.insert(key.to_string(), Box::<T>::new(value));
        } else {
            let data = self.data.get_mut(key).unwrap();
            *data = Box::<T>::new(value);
        }
        self.notify(key);
    }

    fn get<T: 'static>(&self, key: &str) -> Result<&T, String> {
        let p_value = self.data.get(key);
        match p_value {
            Some(v) => match v.downcast_ref::<T>() {
                Some(v) => Ok(v),
                None => Err(format!("Failed to downcast value for key: {}", key)),
            },
            None => Err(format!("Key not found: {}", key)),
        }
    }

    fn reset(&mut self) {
        self.data.clear();
    }
}

static SINGLETON: OnceCell<Mutex<Option<BlackBoardData>>> = OnceCell::new();

fn get_singleton() -> &'static Mutex<Option<BlackBoardData>> {
    SINGLETON.get_or_init(|| {
        trace!("Creating singleton");
        Mutex::new(None)
    })
}

fn start_server(
    _caps: &interfaces::bindings::Capabilities,
    attributes: *const c_char,
) -> Result<(), String> {
    let mut blackboard_data = get_singleton().lock().unwrap();
    if blackboard_data.is_some() {
        return Err("Server is already running".to_string());
    }

    *blackboard_data = Some(BlackBoardData::new());

    if !attributes.is_null() {
        let attributes = unsafe { CStr::from_ptr(attributes).to_str().unwrap() };
        trace!("Attributes: {}", attributes);
        serde_yml::from_str(attributes)
            .map_err(|e| format!("Failed to parse attributes: {}", e))
            .and_then(|entries: Vec<BlackboardEntry>| {
                // String(String),
                // Int(i32),
                // Float(f32),
                // Double(f64),
                // Bool(bool),
                for entry in entries {
                    match entry.value {
                        BlackboardValue::String(v) => {
                            &blackboard_data.as_mut().unwrap().set(entry.key.as_str(), v)
                        }
                        BlackboardValue::Int(v) => {
                            &blackboard_data.as_mut().unwrap().set(entry.key.as_str(), v)
                        }
                        BlackboardValue::Float(v) => {
                            &blackboard_data.as_mut().unwrap().set(entry.key.as_str(), v)
                        }
                        BlackboardValue::Double(v) => {
                            &blackboard_data.as_mut().unwrap().set(entry.key.as_str(), v)
                        }
                        BlackboardValue::Bool(v) => {
                            &blackboard_data.as_mut().unwrap().set(entry.key.as_str(), v)
                        }
                    };
                }
                Ok(())
            })?;
    }
    info!("Blackboard is up and running");
    Ok(())
}

#[no_mangle]
pub extern "C" fn start(
    caps: &interfaces::bindings::Capabilities,
    attributes: *const c_char,
) -> c_int {
    env_logger::init();
    debug!("Starting server");
    match start_server(caps, attributes) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to start server: {}", e);
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn stop() -> c_int {
    debug!("Stopping server");
    let mut blackboard_data = get_singleton().lock().unwrap();
    *blackboard_data = None;
    info!("Blackboard is stopped");
    0
}

#[no_mangle]
pub extern "C" fn summary() -> *const c_char {
    // summary message + null terminator
    SUMMARY_MESSAGE.as_ptr() as *const c_char
}

fn reset_intern() -> Result<(), String> {
    let mut blackboard_data = get_singleton().lock().unwrap();
    if blackboard_data.is_none() {
        return Err("Server is not running".to_string());
    }
    blackboard_data.as_mut().unwrap().reset();
    Ok(())
}

#[no_mangle]
pub extern "C" fn reset() -> c_int {
    match reset_intern() {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to reset server: {}", e);
            -1
        }
    }
}

fn size_intern() -> Result<usize, String> {
    let blackboard_data = get_singleton().lock().unwrap();
    if blackboard_data.is_none() {
        return Err("Server is not running".to_string());
    }

    Ok(blackboard_data.as_ref().unwrap().data.len())
}

#[no_mangle]
pub extern "C" fn size() -> c_int {
    match size_intern() {
        Ok(size) => size as c_int,
        Err(e) => {
            error!("Failed to get size: {}", e);
            -1
        }
    }
}

fn set_string_intern(ckey: *const c_char, cvalue: *const c_char) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    if cvalue.is_null() {
        return Err("Input value is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };
    let value = unsafe { CStr::from_ptr(cvalue).to_str().unwrap() };

    {
        let mut blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        blackboard_data
            .as_mut()
            .unwrap()
            .set(key, value.to_string());
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn set_string(ckey: *const c_char, cvalue: *const c_char) -> c_int {
    match set_string_intern(ckey, cvalue) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to set string: {}", e);
            -1
        }
    }
}

fn get_string_intern(ckey: *const c_char, cvalue: *mut c_char) -> Result<i32, String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        if !blackboard_data.as_ref().unwrap().is_key_valid(key) {
            return Err(format!("Key not found: {}", key));
        }

        let v = blackboard_data.as_ref().unwrap().get::<String>(key);

        match v {
            Ok(v) => {
                if !cvalue.is_null() {
                    let tmp_value = v.as_bytes();
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            tmp_value.as_ptr(),
                            cvalue as *mut u8,
                            tmp_value.len(),
                        );
                    }
                }
                return Ok(v.len() as i32 + 1);
            }
            Err(e) => {
                return Err(format!("Error: {}", e));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn get_string(ckey: *const c_char, cvalue: *mut c_char) -> c_int {
    match get_string_intern(ckey, cvalue) {
        Ok(size) => size,
        Err(e) => {
            error!("Failed to get string: {}", e);
            -1
        }
    }
}

fn get_int_intern(ckey: *const c_char, value: *mut c_int) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    if value.is_null() {
        return Err("Output value is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        if !blackboard_data.as_ref().unwrap().is_key_valid(key) {
            return Err(format!("Key not found: {}", key));
        }

        let v = blackboard_data.as_ref().unwrap().get::<i32>(key);

        match v {
            Ok(v) => {
                unsafe {
                    *value = *v as c_int;
                }
                return Ok(());
            }
            Err(e) => {
                return Err(format!("Error: {}", e));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn get_int(ckey: *const c_char, value: *mut c_int) -> c_int {
    match get_int_intern(ckey, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to get int: {}", e);
            -1
        }
    }
}

fn set_int_intern(ckey: *const c_char, value: c_int) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let mut blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        blackboard_data.as_mut().unwrap().set(key, value);
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn set_int(ckey: *const c_char, value: c_int) -> c_int {
    match set_int_intern(ckey, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to set int: {}", e);
            -1
        }
    }
}

fn get_float_intern(ckey: *const c_char, value: *mut f32) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    if value.is_null() {
        return Err("Output value is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        if !blackboard_data.as_ref().unwrap().is_key_valid(key) {
            return Err(format!("Key not found: {}", key));
        }

        let v = blackboard_data.as_ref().unwrap().get::<f32>(key);

        match v {
            Ok(v) => {
                unsafe {
                    *value = *v;
                }
                return Ok(());
            }
            Err(e) => {
                return Err(format!("Error: {}", e));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn get_float(key: *const c_char, value: *mut f32) -> c_int {
    match get_float_intern(key, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to get float: {}", e);
            -1
        }
    }
}

fn set_float_intern(ckey: *const c_char, value: f32) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let mut blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        blackboard_data.as_mut().unwrap().set(key, value);
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn set_float(key: *const c_char, value: f32) -> c_int {
    match set_float_intern(key, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to set float: {}", e);
            -1
        }
    }
}

fn get_bool_intern(ckey: *const c_char, value: *mut bool) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    if value.is_null() {
        return Err("Output value is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        if !blackboard_data.as_ref().unwrap().is_key_valid(key) {
            return Err(format!("Key not found: {}", key));
        }

        let v = blackboard_data.as_ref().unwrap().get::<bool>(key);

        match v {
            Ok(v) => {
                unsafe {
                    *value = *v;
                }
                return Ok(());
            }
            Err(e) => {
                return Err(format!("Error: {}", e));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn get_bool(key: *const c_char, value: *mut bool) -> c_int {
    match get_bool_intern(key, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to get bool: {}", e);
            -1
        }
    }
}

fn set_bool_intern(ckey: *const c_char, value: bool) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let mut blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        blackboard_data.as_mut().unwrap().set(key, value);
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn set_bool(key: *const c_char, value: bool) -> c_int {
    match set_bool_intern(key, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to set bool: {}", e);
            -1
        }
    }
}

fn get_double_intern(ckey: *const c_char, value: *mut f64) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    if value.is_null() {
        return Err("Output value is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        if !blackboard_data.as_ref().unwrap().is_key_valid(key) {
            return Err(format!("Key not found: {}", key));
        }

        let v = blackboard_data.as_ref().unwrap().get::<f64>(key);

        match v {
            Ok(v) => {
                unsafe {
                    *value = *v;
                }
                return Ok(());
            }
            Err(e) => {
                return Err(format!("Error: {}", e));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn get_double(key: *const c_char, value: *mut f64) -> c_int {
    match get_double_intern(key, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to get double: {}", e);
            -1
        }
    }
}

fn set_double_intern(ckey: *const c_char, value: f64) -> Result<(), String> {
    if ckey.is_null() {
        return Err("Input key is null pointer".to_string());
    }

    let key = unsafe { CStr::from_ptr(ckey).to_str().unwrap() };

    {
        let mut blackboard_data = get_singleton().lock().unwrap();
        if blackboard_data.is_none() {
            return Err("Server is not running".to_string());
        }
        blackboard_data.as_mut().unwrap().set(key, value);
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn set_double(key: *const c_char, value: f64) -> c_int {
    match set_double_intern(key, value) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to set double: {}", e);
            -1
        }
    }
}

fn as_json_schema_intern(cvalue: *mut c_char) -> Result<i32, String> {
    let blackboard_data = get_singleton().lock().unwrap();
    if blackboard_data.is_none() {
        return Err("Server is not running".to_string());
    }

    let mut schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {}
    });

    for (key, value) in blackboard_data.as_ref().unwrap().data.iter() {
        let mut property = serde_json::json!({});
        if let Some(v) = value.downcast_ref::<String>() {
            property["type"] = "string".into();
            property["value"] = v.clone().into();
        } else if let Some(v) = value.downcast_ref::<i32>() {
            property["type"] = "integer".into();
            property["value"] = v.clone().into();
        } else if let Some(v) = value.downcast_ref::<f32>() {
            property["type"] = "number".into();
            property["value"] = v.clone().into();
        } else if let Some(v) = value.downcast_ref::<f64>() {
            property["type"] = "number".into();
            property["value"] = v.clone().into();
        } else if let Some(v) = value.downcast_ref::<bool>() {
            property["type"] = "boolean".into();
            property["value"] = v.clone().into();
        } else {
            return Err(format!("Unsupported type for key: {}", key));
        }
        schema["properties"][key] = property;
    }

    let schema_str = schema.to_string() + "\0";

    if !cvalue.is_null() {
        let tmp_value = schema_str.as_bytes();
        unsafe {
            std::ptr::copy_nonoverlapping(tmp_value.as_ptr(), cvalue as *mut u8, tmp_value.len());
        }
    }
    return Ok(schema_str.len() as i32);
}

#[no_mangle]
pub extern "C" fn as_json_schema(value: *mut c_char) -> c_int {
    match as_json_schema_intern(value) {
        Ok(size) => size,
        Err(e) => {
            error!("Failed to get json schema: {}", e);
            -1
        }
    }
}

fn subscribe_intern(
    key: *const c_char,
    component: *const c_char,
    callback: *mut c_void,
    user_data: *mut c_void,
) -> Result<(), String> {
    let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
    let component = unsafe { CStr::from_ptr(component).to_str().unwrap() };

    let mut blackboard_data = get_singleton().lock().unwrap();
    if blackboard_data.is_none() {
        return Err("Server is not running".to_string());
    }

    blackboard_data
        .as_mut()
        .unwrap()
        .subscribe(key, component, callback, user_data);
    Ok(())
}

#[no_mangle]
pub extern "C" fn subscribe(
    key: *const c_char,
    component: *const c_char,
    callback: *mut c_void,
    user_data: *mut c_void,
) -> c_int {
    match subscribe_intern(key, component, callback, user_data) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to subscribe: {}", e);
            -1
        }
    }
}

fn unsubscribe_intern(key: *const c_char, component: *const c_char) -> Result<(), String> {
    let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
    let component = unsafe { CStr::from_ptr(component).to_str().unwrap() };

    let mut blackboard_data = get_singleton().lock().unwrap();
    if blackboard_data.is_none() {
        return Err("Server is not running".to_string());
    }

    blackboard_data.as_mut().unwrap().unsubscribe(key, component);
    Ok(())
}

#[no_mangle]
pub extern "C" fn unsubscribe(key: *const c_char, component: *const c_char) -> c_int {
    match unsubscribe_intern(key, component) {
        Ok(_) => 0,
        Err(e) => {
            error!("Failed to unsubscribe: {}", e);
            -1
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;
    use std::time::Duration;
    use super::*;
    use assert_float_eq::assert_f32_near;
    use rstest::fixture;
    use rstest::rstest;
    use serial_test::serial;
    use std::sync::mpsc;

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_start() {
        let key_values: Vec<BlackboardEntry> = vec![
            BlackboardEntry {
                key: "StringValue".to_string(),
                value: BlackboardValue::String("Hello, World!".to_string()),
            },
            BlackboardEntry {
                key: "IntValue".to_string(),
                value: BlackboardValue::Int(42),
            },
        ];

        let attributes = serde_yml::to_string(&key_values).unwrap() + "\0";

        debug!("Attributes: {}", attributes);

        let caps = interfaces::capabilities::Capabilities::new();
        let _result = stop();
        let result = start_server(caps.inner(), attributes.as_ptr() as *const c_char);
        assert_eq!(result.is_ok(), true);

        {
            let singleton = get_singleton().lock().unwrap();
            assert!(singleton.is_some());
            let singleton = singleton.as_ref().unwrap();
            assert_eq!(singleton.data.len(), 2);
        }

        {
            let singleton = get_singleton().lock().unwrap();
            assert!(singleton.is_some());
            let singleton = singleton.as_ref().unwrap();
            assert_eq!(singleton.data.len(), 2);
        }

        let mut int_value: i32 = 0;
        let result = get_int("IntValue\0".as_ptr() as *const c_char, &mut int_value);
        assert_eq!(result, 0);
        assert_eq!(int_value, 42);

        let string_value_length = get_string(
            "StringValue\0".as_ptr() as *const c_char,
            std::ptr::null_mut(),
        );
        assert!(string_value_length > 0);

        let mut buffer = vec![0u8; string_value_length as usize];
        let string_value_length = get_string(
            "StringValue\0".as_ptr() as *const c_char,
            buffer.as_mut_ptr() as *mut c_char,
        );

        assert!(string_value_length > 0);

        let string_value = unsafe { std::str::from_utf8_unchecked(&buffer) };
        assert_eq!(string_value, "Hello, World!\0");

        let result = stop();
        assert_eq!(result, 0);

        {
            let singleton = get_singleton().lock().unwrap();
            assert!(singleton.is_none());
        }
    }

    #[fixture]
    fn startup() -> c_int {
        let _result = stop();
        let caps = interfaces::capabilities::Capabilities::new();
        let result = start_server(caps.inner(), std::ptr::null());
        return if result.is_ok() { 0 } else { -1 };
    }

    #[serial]
    #[test]
    fn test_string() {
        let key = "int_key_4\0";
        let ckey = key.as_ptr() as *const c_char;

        unsafe {
            let y = CStr::from_ptr(ckey).to_str().unwrap();

            assert_eq!(&key[0..key.len() - 1], y);
        }
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_set_int(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "int_key_4\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = 42;

        let result = set_int(key_c, value);
        assert_eq!(result, 0);

        let mut return_value = 0;

        let result = get_int(key_c, &mut return_value);
        assert_eq!(result, 0);
        assert_eq!(value, return_value);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_int_not_found(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "int_key_not_found\0";
        let key_c = key.as_ptr() as *const c_char;
        let mut return_value = 0;
        let result = get_int(key_c, &mut return_value);
        assert_eq!(result, -1);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_set_float(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "float_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = 42.0;

        let result = set_float(key_c, value);
        assert_eq!(result, 0);

        let mut return_value = 0.0;

        let result = get_float(key_c, &mut return_value);
        assert_eq!(result, 0);
        assert_f32_near!(value, return_value);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_float_not_found(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "float_key_not_found\0";
        let key_c = key.as_ptr() as *const c_char;
        let mut return_value = 0.0;

        let result = get_float(key_c, &mut return_value);
        assert_eq!(result, -1);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_set_bool(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "bool_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = true;

        let result = set_bool(key_c, value);
        assert_eq!(result, 0);

        let mut result_value = false;

        let result = get_bool(key_c, &mut result_value);
        assert_eq!(result, 0);
        assert_eq!(result_value, value);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_bool_not_found(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "bool_key_not_found\0";
        let key_c = key.as_ptr() as *const c_char;

        let mut result_value = false;
        let result = get_bool(key_c, &mut result_value);
        assert_eq!(result, -1);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_set_double(startup: c_int) {
        assert_eq!(startup, 0);

        let key = "double_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = 42.0;

        let result = set_double(key_c, value);
        assert_eq!(result, 0);

        let mut result_value = 0.0;
        let result = get_double(key_c, &mut result_value);
        assert_eq!(result, 0);
        assert_eq!(result_value, value);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_double_not_found(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "double_key_not_found\0";
        let key_c = key.as_ptr() as *const c_char;
        let mut result_value = 0.0;
        let result = get_double(key_c, &mut result_value);
        assert_eq!(result, -1);
    }

    #[serial]
    #[test_log::test]
    fn test_summary() {
        // memory leak
        let result = &String::from(SUMMARY_MESSAGE)[0..SUMMARY_MESSAGE.len() - 1]; // remove null terminator
        let summary_result_c = summary();
        let summary_result = unsafe { CStr::from_ptr(summary_result_c).to_str().unwrap() };
        assert_eq!(result, summary_result);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_set_string(startup: c_int) {
        assert_eq!(startup, 0);

        let key = "key\0";
        let value = "value\0";
        let key_c = key.as_ptr() as *const c_char;
        let value_c = value.as_ptr() as *const c_char;

        let result = set_string(key_c, value_c);
        assert_eq!(result, 0);

        let size = get_string(key_c, std::ptr::null_mut());
        assert_eq!(size, value.len() as i32);

        let mut buffer = vec![0u8; value.len()];

        let result = get_string(key_c, buffer.as_mut_ptr() as *mut c_char);
        assert_eq!(result, value.len() as i32);

        let result_str = unsafe { std::str::from_utf8_unchecked(&buffer) };
        assert_eq!(result_str, value);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_get_string_not_found(startup: c_int) {
        assert_eq!(startup, 0);
        let key = "key_not_found\0";
        let key_c = key.as_ptr() as *const c_char;

        let result = get_string(key_c, std::ptr::null_mut());
        assert_eq!(result, -1);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_reset(startup: c_int) {
        assert_eq!(startup, 0);
        assert_eq!(size(), 0);
        let key = "int_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = 42;

        let result = set_int(key_c, value);
        assert_eq!(result, 0);
        let mut result_value = 0;
        let result = get_int(key_c, &mut result_value);
        assert_eq!(result, 0);
        assert_eq!(result_value, value);
        assert_eq!(size(), 1);

        reset();
        assert_eq!(size(), 0);
        let mut result_value = 0;
        let result = get_int(key_c, &mut result_value);
        assert_eq!(result, -1);
    }

    

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_subscribe(startup: c_int) {
        assert_eq!(startup, 0);

        static mut CALLBACK_CALLED: bool = false;

        extern "C" fn callback(key: *const c_char, user_data: *mut c_void) -> c_int {
            let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
            debug!("Callback called for key: {}", key);
            unsafe {
                CALLBACK_CALLED = true;
            }
            0
        }
        
        let key = "int_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let component = "component\0";
        let component_c = component.as_ptr() as *const c_char;

        let result = subscribe_intern(key_c, component_c, callback as *mut c_void, std::ptr::null_mut());
        assert_eq!(result.is_ok(), true);
        let callback_called = unsafe { CALLBACK_CALLED };
        assert_eq!(callback_called, false);
        let set_value = 42;
        let result = set_int(key_c, set_value);
        assert_eq!(result, 0);
        let callback_called = unsafe { CALLBACK_CALLED };
        assert_eq!(callback_called, true);

        let result = unsubscribe_intern(key_c, component_c);
        assert_eq!(result.is_ok(), true);

    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_subscribe_with_user_data(startup: c_int) {
        assert_eq!(startup, 0);

        
        let (sender, receiver): (mpsc::Sender<String>, mpsc::Receiver<String>) = mpsc::channel();
        let sender_ptr = Box::into_raw(Box::new(sender));

        extern "C" fn callback(key: *const c_char, user_data: *mut c_void) -> c_int {
            let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
            debug!("Callback called for key: {}", key);

            if user_data.is_null() {
                error!("User data is null");
                return -1;
            }

            let sender = unsafe { &*(user_data as *mut mpsc::Sender<String>) };

            sender.send(key.to_string()).unwrap_or_else(|e| {
                error!("Failed to send key: {}", key);
            }
            );
            0
        }
        
        let key = "int_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let component = "component\0";
        let component_c = component.as_ptr() as *const c_char;

        let result = subscribe_intern(key_c, component_c, callback as *mut c_void, sender_ptr as *mut c_void);
        assert_eq!(result.is_ok(), true);

        let set_value = 42;
        let result = set_int(key_c, set_value);
        assert_eq!(result, 0);

        assert_eq!(receiver.recv_timeout(Duration::from_secs(1)).is_ok(), true);

        let set_value = 43;
        let result = set_int(key_c, set_value);
        assert_eq!(result, 0);

        assert_eq!(receiver.recv_timeout(Duration::from_secs(1)).is_ok(), true);

        let set_value = 60;
        let result = set_int(key_c, set_value);
        assert_eq!(result, 0);

        assert_eq!(receiver.recv_timeout(Duration::from_secs(1)).is_ok(), true);
        
        let result = unsubscribe_intern(key_c, component_c);
        assert_eq!(result.is_ok(), true);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_json_schema(startup: c_int) {
        assert_eq!(startup, 0);

        let key = "int_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = 42;
        let result = set_int(key_c, value);

        assert_eq!(result, 0);

        let key = "string_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = "Hello, World!\0";
        let value_c = value.as_ptr() as *const c_char;
        let result = set_string(key_c, value_c);

        assert_eq!(result, 0);

        let key = "float_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = 42.0;
        let result = set_float(key_c, value);

        assert_eq!(result, 0);

        let key = "double_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = 42.0;
        let result = set_double(key_c, value);

        assert_eq!(result, 0);

        let key = "bool_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = true;
        let result = set_bool(key_c, value);

        assert_eq!(result, 0);

        let buffer_size = as_json_schema(std::ptr::null_mut());
        assert!(buffer_size > 0);

        let mut buffer = vec![0u8; buffer_size as usize];
        let buffer_size = as_json_schema(buffer.as_mut_ptr() as *mut c_char);
        assert!(buffer_size > 0);

        debug!("Buffer size: {}", buffer_size);

        let schema = unsafe {
            CStr::from_ptr(buffer.as_ptr() as *const c_char)
                .to_str()
                .unwrap()
        };
        debug!("Schema: {}", schema);
    }

    #[rstest]
    #[serial]
    #[test_log::test]
    fn test_error_case_set_string_try_to_get_int(startup: c_int)
    {
        assert_eq!(startup, 0);

        let key = "string_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let value = "Hello, World!\0";
        let value_c = value.as_ptr() as *const c_char;
        let result = set_string(key_c, value_c);

        assert_eq!(result, 0);

        let key = "string_key\0";
        let key_c = key.as_ptr() as *const c_char;
        let mut value =0;
        let result = get_int(key_c, &mut value);

        assert_eq!(result, -1);

    }

}
