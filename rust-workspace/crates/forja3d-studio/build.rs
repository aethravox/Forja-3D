use std::{env, path::PathBuf};

const JAVA_FILE_RELATIVE_PATH: &str = "src/android/SystemBarsHelper.java";

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    #[cfg(windows)]
    if target_os == "windows" {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(err) = res.compile() {
            println!("cargo:warning=winresource failed: {err}");
        }
    }

    if target_os == "android" {
        println!("cargo:rerun-if-changed={JAVA_FILE_RELATIVE_PATH}");
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let java_file =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join(JAVA_FILE_RELATIVE_PATH);
        let android_jar_path =
            android_build::android_jar(None).expect("Failed to find android.jar");

        assert!(
            android_build::JavaBuild::new()
                .class_path(android_jar_path.clone())
                .classes_out_dir(out_dir.clone())
                .file(java_file)
                .compile()
                .expect("failed to execute javac for SystemBarsHelper.java")
                .success(),
            "javac failed for SystemBarsHelper.java"
        );

        let class_dir = out_dir.join("dev").join("aethravox").join("forja3d");
        let class_files: Vec<PathBuf> = std::fs::read_dir(&class_dir)
            .expect("failed to read compiled class directory")
            .filter_map(|e| e.ok().map(|entry| entry.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("class"))
            .collect();
        assert!(!class_files.is_empty(), "no .class files produced by javac");

        let d8_jar_path = android_build::android_d8_jar(None).expect("Failed to find d8.jar");
        let mut d8_cmd = android_build::JavaRun::new();
        d8_cmd
            .class_path(d8_jar_path)
            .main_class("com.android.tools.r8.D8")
            .arg("--classpath")
            .arg(android_jar_path)
            .arg("--min-api")
            .arg("26")
            .arg("--output")
            .arg(&out_dir);
        for class_file in &class_files {
            d8_cmd.arg(class_file);
        }
        assert!(
            d8_cmd
                .run()
                .expect("failed to execute d8 for SystemBarsHelper")
                .success(),
            "d8 failed for SystemBarsHelper"
        );
    }
}
