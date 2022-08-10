fn main(){
    cfg_if::cfg_if! {
        if #[cfg(feature = "occlum")] {
            println!("cargo:rustc-link-search=deps/rats-tls/build-occlum/src");
            println!("cargo:rustc-link-lib=rats_tls");
            // We specify `-rpath` here because it only works in binary crate and not in library crate.
            // See: https://github.com/rust-lang/cargo/issues/5077#issuecomment-912895057
            println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/local/lib/rats-tls");
        }else if #[cfg(feature = "host")] {
            println!("cargo:rustc-link-search=/usr/local/lib/rats-tls");
            println!("cargo:rustc-link-lib=rats_tls");
            println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/local/lib/rats-tls");
        }else {
            panic!("One of these features must be specified: {:?}", ["host", "occlum"]);
        }
    }
}