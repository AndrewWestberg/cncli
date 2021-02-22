use std::process::Command;

macro_rules! ok (($expression:expr) => ($expression.unwrap()));
macro_rules! log {
    ($fmt:expr) => (println!(concat!("cncli/build.rs:{}: ", $fmt), line!()));
    ($fmt:expr, $($arg:tt)*) => (println!(concat!("cncli/build.rs:{}: ", $fmt),
    line!(), $($arg)*));
}

fn main() {
    // Build and link IOHK libsodium
    run("git", |command| {
        command
            .arg("submodule")
            .arg("update")
            .arg("--init")
            .arg("--recursive")
            .arg("--force")
    });

    // Build contrib automatically (as part of rust build)
    #[cfg(not(feature = "external-contrib-build"))]
    {
        let libsodium = autotools::Config::new("contrib/libsodium/").reconf("-vfi").build();
        println!("cargo:rustc-link-search=native={}", libsodium.join("lib").display());
        println!("cargo:rustc-link-lib=static=sodium");
    }

    // Expect contrib to be pre-built (external to rust build) - necessary when using
    // MSYS2/MinGW64 to build.
    #[cfg(feature = "external-contrib-build")]
    {
        println!("cargo:rustc-link-search=contrib/libsodium/src/libsodium/.libs");
    }

    println!("cargo:return-if-changed=build.rs");
}

fn run<F>(name: &str, mut configure: F)
where
    F: FnMut(&mut Command) -> &mut Command,
{
    let mut command = Command::new(name);
    let configured = configure(&mut command);
    log!("Executing {:?}", configured);
    if !ok!(configured.status()).success() {
        panic!("failed to execute {:?}", configured);
    }
    log!("Command {:?} finished successfully", configured);
}
