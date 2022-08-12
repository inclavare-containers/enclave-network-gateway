fn main() {
    // Currently Enta will only link to host mode librats_tls.so
    println!("cargo:rustc-link-search=/usr/local/lib/rats-tls");
    println!("cargo:rustc-link-lib=rats_tls");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/local/lib/rats-tls");
}
