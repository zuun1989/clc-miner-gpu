pub async fn report(server: &str, user: &str, speed: &f64, total_mined: &f64, best: &str) -> String {
    if server == "" {
        return String::from("");
    }
    let url = format!("{}/report?user={}&speed={}&best={}&mined={}", server, user, speed, best, total_mined);
    match reqwest::get(&url).await {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    }
}
