use std::{os::raw::c_void, marker, iter};
use crate::bindings::{self, CAPABILITY_FUNCTION_NAME_LEN};

// reimplementation of libloading::Function to allow custom getter
pub struct Function<T> { // we admit here that the lifetime of the function is less than the lifetime of the library
    pointer: *mut c_void,
    pd: marker::PhantomData<T>,
}

impl<T> ::std::ops::Deref for Function<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe {
            // Additional reference level for a dereference on `deref` return value.
            &*(&self.pointer as *const *mut _ as *const T)
        }
    }
}

unsafe impl<T: Send> Send for Function<T> {}
unsafe impl<T: Sync> Sync for Function<T>  {}

impl <T> Clone for Function<T> {
    fn clone(&self) -> Self {
        Function {
            pointer: self.pointer.clone(),
            pd: marker::PhantomData,
        }
    }
}

pub struct Capability (bindings::Capability);


fn capability_name(cap: &bindings::Capability) -> String {
    let mut name = String::new();
    for i in 0..CAPABILITY_FUNCTION_NAME_LEN as usize {
        if cap.name[i] == 0 {
            break;
        }
        name.push(cap.name[i] as u8 as char);
    }
    name
}

unsafe impl Send for Capability {}

impl Capability {
    pub fn new(name: &str, function: *mut c_void) -> Self {
        let mut cap = bindings::Capability {
            name: [0; CAPABILITY_FUNCTION_NAME_LEN as usize],
            function: function,
        };
        let name_bytes = name.as_bytes();

        let name_len = if name_bytes.len() + 1 > CAPABILITY_FUNCTION_NAME_LEN as usize {
            CAPABILITY_FUNCTION_NAME_LEN as usize - 1 // leave space for null terminator
        } else {
            name_bytes.len()
        };
        for i in 0..name_len {
            cap.name[i] = name_bytes[i] as i8;
        }

        Capability(cap)
    }

    pub fn from_raw(cap: &bindings::Capability) -> Self {
        Capability(cap.clone())
    }

    pub fn name(&self) -> String {
        capability_name(&self.0)
    }

    pub unsafe fn get<T>(&self) -> Result<Function<T>, String> {
        let function = self.0.function;
        if function.is_null() {
            return Err("Function pointer is null".to_string());
        }
        Ok(Function {
            pointer: function,
            pd: marker::PhantomData,
        })
    }

    pub fn inner(&self) -> &bindings::Capability {
        &self.0
    }

}

#[derive(Debug)]
pub struct Capabilities (bindings::Capabilities);

impl Capabilities {
    pub fn new() -> Self {
        Capabilities(bindings::Capabilities {
            capability: [Capability::new("", std::ptr::null_mut()).inner().clone(); 20],
            n_capabilities: 0,
        })
    }

    pub fn from_raw(cap: &bindings::Capabilities) -> Self {
        Capabilities(cap.clone())
    }

    pub fn add(&mut self, cap: Capability) {
        if self.0.n_capabilities < 20 {
            self.0.capability[self.0.n_capabilities as usize] = cap.inner().clone();
            self.0.n_capabilities += 1;
        }
    }

    pub fn get(&self, name: &str) -> Option<Capability> {
        for i in 0..self.0.n_capabilities {
            let cap = &self.0.capability[i as usize];
            let cap_name = capability_name(cap);
            if cap_name.len() != name.len() {
                continue;
            }
            if cap_name == name {
                return Some(Capability::from_raw(cap));
            }
        }
        None
    }

    pub fn inner(&self) -> &bindings::Capabilities {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.n_capabilities as usize
    }

    pub fn iter(&self) -> CapabilitiesIterator {
        CapabilitiesIterator {
            capabilities: self,
            index: 0,
        }
    }

}

pub struct CapabilitiesIterator<'a> {
    capabilities: &'a Capabilities,
    index: usize,
}

impl<'a> iter::Iterator for CapabilitiesIterator<'a> {
    type Item = Capability;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.capabilities.len() {
            let cap = &self.capabilities.0.capability[self.index];
            self.index += 1;
            Some(Capability::from_raw(cap))
        } else {
            None
        }
    }
}

unsafe impl Send for Capabilities {}