use std::{
    ffi::{c_char, CString},
    ptr::null_mut,
    str::from_utf8,
};

openvm::entry!(main);

#[no_mangle]
pub extern "C" fn coremark_putchar(c: u8) {
    let b = [c];
    if let Ok(s) = from_utf8(&b) {
        openvm::io::print(s);
    } else {
        openvm::io::print("?");
    }
}

extern "C" {
    fn coremark_main(argc: i32, argv: *mut *mut c_char) -> i32;
}

fn main() -> Result<(), i32> {
    let arg0 = CString::new("coremark").unwrap();
    let mut argv: Vec<*mut c_char> = vec![arg0.as_ptr() as *mut c_char, null_mut()];
    let rc = unsafe { coremark_main(1, argv.as_mut_ptr()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(rc)
    }
}
