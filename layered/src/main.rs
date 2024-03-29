#[cfg_attr(feature = "jemalloc", global_allocator)]
#[cfg(feature = "jemalloc")]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

const SRC: &str = "0.0.0.0:5555";
const DST: &str = "127.0.0.1:5555";

fn main() {
    let dst = std::env::var("IP")
        .map(|ip| format!("{}:5555", ip))
        .unwrap_or_else(|_| DST.into());
    match std::env::args().nth(1).as_ref().map(String::as_ref) {
        Some("client") => febft::bench::layered_bench(febft::bench::Side::Client, &dst),
        Some("server") => febft::bench::layered_bench(febft::bench::Side::Server, SRC),
        _ => panic!("Arg must be either client or server"),
    }
}
