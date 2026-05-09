use indicatif::ProgressStyle;

pub fn get_api_url() -> String {
    std::env::var("TRYTET_API_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

pub fn pb_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.cyan} {msg} [{elapsed_precise}]")
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
}
