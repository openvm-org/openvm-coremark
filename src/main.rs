use openvm::io::reveal_u32;

extern "C" {
    fn add_u32(a: u32, b: u32) -> u32;
}

fn main() {
    let a = 8u32;
    let b = 3u32;
    let res = unsafe { add_u32(a, b) };
    reveal_u32(res, 0);
}
