#[macro_export]
macro_rules! wrapper {
    // Single expression (like a function name or closure)
    ($f:expr) => {{
        $f()
    }};
    ($f:expr, $( $args:expr $(,)? )* ) => {{
        $f( $($args,)* )
    }};
}
#[macro_export]
macro_rules! retry {
    ($( $args:expr$(,)? )+) => {{
        let mut wait: u64 = 5;
        loop {
            let res = wrapper!($( $args, )*);
            if res.is_ok() {
                break res;
            }
            if wait <= 20 {
                thread::sleep(std::time::Duration::from_secs(wait));
                wait *= 2;
                continue;
            }
            break res;
        }
    }};
}
