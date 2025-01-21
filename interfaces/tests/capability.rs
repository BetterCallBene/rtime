use libloading::{Library, Symbol};
use interfaces::capabilities::{Capability, Capabilities, Function};

const TARGET_DIR: Option<&'static str> = option_env!("CARGO_TARGET_DIR");
const TARGET_TMPDIR: Option<&'static str> = option_env!("CARGO_TARGET_TMPDIR");

fn lib_path() -> std::path::PathBuf {
    [
        TARGET_TMPDIR.unwrap_or(TARGET_DIR.unwrap_or("target")),
        "libtest_helpers.module",
    ]
    .iter()
    .collect()
}

fn make_helpers() {
    static ONCE: ::std::sync::Once = ::std::sync::Once::new();
    ONCE.call_once(|| {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let mut cmd = ::std::process::Command::new(rustc);
        cmd.arg("src/test_helpers.rs").arg("-o").arg(lib_path());
        if let Some(target) = std::env::var_os("TARGET") {
            cmd.arg("--target").arg(target);
        } else {
            eprintln!("WARNING: $TARGET NOT SPECIFIED! BUILDING HELPER MODULE FOR NATIVE TARGET.");
        }
        assert!(cmd
            .status()
            .expect("could not compile the test helpers!")
            .success());
    });
}

#[test]
fn test_id_u32() {
    make_helpers();
    unsafe {
        let lib = Library::new(lib_path()).unwrap();
        let f: Symbol<unsafe extern "C" fn(u32) -> u32> = lib.get(b"test_identity_u32\0").unwrap();

        let cap = Capability::new("test_identity_u32", f.try_as_raw_ptr().unwrap());
        assert_eq!(cap.name(), "test_identity_u32");

        let f2: Function<unsafe extern "C" fn(u32) -> u32> = cap.get().unwrap();

        assert_eq!(42, f2(42));
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
struct S {
    a: u64,
    b: u32,
    c: u16,
    d: u8,
}

#[test]
fn test_create_capabilties() {
    make_helpers();
    
    unsafe {
        let lib = Library::new(lib_path()).unwrap();
        
        let test_identity_u32_fn: Symbol<unsafe extern "C" fn(u32) -> u32> = lib.get(b"test_identity_u32\0").unwrap();
        let test_identity_struct_fn: Symbol<unsafe extern "C" fn(S) -> S> = lib.get(b"test_identity_struct\0").unwrap();

        let cap1 = Capability::new("test_identity_u32", test_identity_u32_fn.try_as_raw_ptr().unwrap());
        let cap2 = Capability::new("test_identity_struct", test_identity_struct_fn.try_as_raw_ptr().unwrap());

        let capabilities = vec![cap1, cap2];

        let mut caps = Capabilities::new();
        for cap in capabilities {
            caps.add(cap);
        }

        assert_eq!(caps.len(), 2);

        let cap1 = caps.get("test_identity_u32").unwrap();
        let f: Function<unsafe extern "C" fn(u32) -> u32> = cap1.get().unwrap();
        assert_eq!(42, f(42));

        let cap2 = caps.get("test_identity_struct").unwrap();

        let s = S {
            a: 42,
            b: 42,
            c: 42,
            d: 42,
        };

        let f: Function<unsafe extern "C" fn(S) -> S> = cap2.get().unwrap();
        assert_eq!(s, f(s));

        assert_eq!(caps.inner().n_capabilities, 2);

    }
}