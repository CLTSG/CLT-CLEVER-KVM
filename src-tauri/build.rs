fn main() {
    // Create web-client directory if it doesn't exist
    let web_client_dir = std::path::Path::new("web-client");
    if !web_client_dir.exists() {
        std::fs::create_dir_all(web_client_dir).expect("Failed to create web-client directory");
        
        // Create a basic index.html file as a placeholder
        let index_html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Clever KVM</title>
    <style>
        body {
            font-family: sans-serif;
            margin: 0;
            padding: 20px;
            text-align: center;
        }
        h1 {
            color: #2c3e50;
        }
        p {
            color: #7f8c8d;
        }
    </style>
</head>
<body>
    <h1>Clever KVM</h1>
    <p>To access the KVM functionality, use the /kvm endpoint.</p>
    <p>Example: <a href="/kvm">Open KVM Client</a></p>
</body>
</html>
"#;
        
        std::fs::write(web_client_dir.join("index.html"), index_html)
            .expect("Failed to create index.html");
    }
    let kvm_dir = std::path::Path::new("kvm");
    if !kvm_dir.exists() {
        std::fs::create_dir_all(kvm_dir).expect("Failed to create kvm directory");
    }
    
    tauri_build::build()
}
