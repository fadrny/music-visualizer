use base64::Engine;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const POLL: Duration = Duration::from_millis(3000);

#[derive(Default)]
struct State {
    track_name: String,
    artist_name: String,
    pending_album_art: Option<Vec<u8>>,
    last_art_url: String,
    connected: bool,
    auth_in_progress: bool,
    access_token: String,
    refresh_token: String,
    token_expires_at: Option<Instant>,
}

pub struct SpotifyClient {
    client_id: Option<String>,
    state: Arc<Mutex<State>>,
    stop_flag: Arc<Mutex<bool>>,
}

impl SpotifyClient {
    pub fn new() -> Self {
        let client_id = load_client_id();
        Self {
            client_id,
            state: Arc::new(Mutex::new(State::default())),
            stop_flag: Arc::new(Mutex::new(false)),
        }
    }

    pub fn is_connected(&self) -> bool { self.state.lock().unwrap().connected }
    pub fn is_auth_in_progress(&self) -> bool { self.state.lock().unwrap().auth_in_progress }
    pub fn has_track(&self) -> bool { !self.state.lock().unwrap().track_name.is_empty() }
    pub fn track_name(&self) -> String { self.state.lock().unwrap().track_name.clone() }
    pub fn artist_name(&self) -> String { self.state.lock().unwrap().artist_name.clone() }

    pub fn consume_new_album_art(&self) -> Option<Vec<u8>> {
        self.state.lock().unwrap().pending_album_art.take()
    }

    pub fn start_auth(&self) {
        let client_id = match self.client_id.clone() {
            Some(c) => c,
            None => return,
        };
        {
            let mut st = self.state.lock().unwrap();
            if st.auth_in_progress { return; }
            st.auth_in_progress = true;
        }
        let state = self.state.clone();
        let stop_flag = self.stop_flag.clone();

        thread::spawn(move || {
            let verifier = base64_url(&rand_bytes(64));
            let mut hasher = Sha256::new();
            hasher.update(verifier.as_bytes());
            let challenge = base64_url(&hasher.finalize());

            let scope = "user-read-currently-playing user-read-playback-state";
            let auth_url = format!(
                "https://accounts.spotify.com/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge_method=S256&code_challenge={}&scope={}",
                url::form_urlencoded::byte_serialize(client_id.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(REDIRECT_URI.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(challenge.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(scope.as_bytes()).collect::<String>(),
            );

            let server = match tiny_http::Server::http("127.0.0.1:8888") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[Spotify] callback server: {}", e);
                    state.lock().unwrap().auth_in_progress = false;
                    return;
                }
            };

            if let Err(e) = opener::open(&auth_url) {
                println!("[Spotify] open this URL: {} (open error: {})", auth_url, e);
            }

            let deadline = Instant::now() + Duration::from_secs(120);
            let mut code: Option<String> = None;
            while Instant::now() < deadline {
                match server.recv_timeout(Duration::from_millis(500)) {
                    Ok(Some(req)) => {
                        let url_str = req.url().to_string();
                        let query = url_str.split_once('?').map(|x| x.1).unwrap_or("");
                        for pair in query.split('&') {
                            if let Some(v) = pair.strip_prefix("code=") {
                                code = Some(urlencoding_decode(v));
                            }
                        }
                        let body = if code.is_some() {
                            "<h1 style='color:#1DB954'>&#10003; Connected!</h1><p>Return to visualizer.</p>"
                        } else {
                            "<h1 style='color:#f44'>Failed</h1>"
                        };
                        let resp = tiny_http::Response::from_string(body)
                            .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
                        let _ = req.respond(resp);
                        if code.is_some() { break; }
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            drop(server);

            let Some(code) = code else {
                eprintln!("[Spotify] Auth timed out or failed");
                state.lock().unwrap().auth_in_progress = false;
                return;
            };

            // Exchange code for tokens
            let body = format!(
                "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
                url::form_urlencoded::byte_serialize(code.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(REDIRECT_URI.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(client_id.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(verifier.as_bytes()).collect::<String>(),
            );
            let token_resp: Result<TokenResp, _> = ureq::post("https://accounts.spotify.com/api/token")
                .set("Content-Type", "application/x-www-form-urlencoded")
                .send_string(&body)
                .and_then(|r| Ok(r.into_json::<TokenResp>().unwrap_or_default()));

            match token_resp {
                Ok(tk) if !tk.access_token.is_empty() => {
                    {
                        let mut st = state.lock().unwrap();
                        st.access_token = tk.access_token;
                        st.refresh_token = tk.refresh_token;
                        st.token_expires_at = Some(Instant::now() + Duration::from_secs(tk.expires_in.saturating_sub(60) as u64));
                        st.connected = true;
                        st.auth_in_progress = false;
                    }
                    println!("[Spotify] Authenticated!");
                    // Start polling thread
                    let st_poll = state.clone();
                    let stop_poll = stop_flag.clone();
                    let cid_poll = client_id.clone();
                    thread::spawn(move || poll_loop(st_poll, stop_poll, cid_poll));
                }
                Ok(_) => {
                    eprintln!("[Spotify] Token exchange returned empty token");
                    state.lock().unwrap().auth_in_progress = false;
                }
                Err(e) => {
                    eprintln!("[Spotify] Auth failed: {}", e);
                    state.lock().unwrap().auth_in_progress = false;
                }
            }
        });
    }

    pub fn stop(&self) {
        *self.stop_flag.lock().unwrap() = true;
        self.state.lock().unwrap().connected = false;
    }
}

fn poll_loop(state: Arc<Mutex<State>>, stop_flag: Arc<Mutex<bool>>, client_id: String) {
    loop {
        if *stop_flag.lock().unwrap() { return; }

        // Refresh token if expiring
        let needs_refresh = {
            let st = state.lock().unwrap();
            st.token_expires_at.map_or(false, |d| Instant::now() >= d)
        };
        if needs_refresh {
            refresh_token(&state, &client_id);
        }

        let (connected, token) = {
            let st = state.lock().unwrap();
            (st.connected, st.access_token.clone())
        };
        if !connected { thread::sleep(POLL); continue; }

        match ureq::get("https://api.spotify.com/v1/me/player/currently-playing")
            .set("Authorization", &format!("Bearer {}", token))
            .call()
        {
            Ok(resp) => {
                if resp.status() == 204 {
                    let mut st = state.lock().unwrap();
                    st.track_name.clear();
                    st.artist_name.clear();
                } else if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    handle_currently_playing(&state, &json);
                }
            }
            Err(e) => eprintln!("[Spotify] Poll: {}", e),
        }

        thread::sleep(POLL);
    }
}

fn handle_currently_playing(state: &Arc<Mutex<State>>, json: &serde_json::Value) {
    let item = match json.get("item") {
        Some(i) if !i.is_null() => i,
        _ => {
            let mut st = state.lock().unwrap();
            st.track_name.clear();
            st.artist_name.clear();
            return;
        }
    };
    let track_name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let artist_name = item.get("artists")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter()
            .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
            .collect::<Vec<_>>()
            .join(", "))
        .unwrap_or_default();

    let images = item.get("album").and_then(|a| a.get("images")).and_then(|i| i.as_array());
    let best_url = images.and_then(|arr| {
        let mut best: Option<(i64, &str)> = None;
        for img in arr {
            let w = img.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
            let url = img.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() { continue; }
            let diff = (w - 300).abs();
            match best {
                None => best = Some((diff, url)),
                Some((bd, _)) if diff < bd => best = Some((diff, url)),
                _ => {}
            }
        }
        best.map(|(_, u)| u.to_string())
    });

    let download_url = {
        let mut st = state.lock().unwrap();
        st.track_name = track_name;
        st.artist_name = artist_name;
        match best_url {
            Some(url) if url != st.last_art_url => {
                st.last_art_url = url.clone();
                Some(url)
            }
            _ => None,
        }
    };

    if let Some(url) = download_url {
        match ureq::get(&url).call() {
            Ok(resp) if resp.status() == 200 => {
                let mut bytes = Vec::new();
                if resp.into_reader().read_to_end(&mut bytes).is_ok() {
                    state.lock().unwrap().pending_album_art = Some(bytes);
                }
            }
            Ok(resp) => eprintln!("[Spotify] album art status {}", resp.status()),
            Err(e) => eprintln!("[Spotify] album art download: {}", e),
        }
    }
}

fn refresh_token(state: &Arc<Mutex<State>>, client_id: &str) {
    let refresh = state.lock().unwrap().refresh_token.clone();
    if refresh.is_empty() { return; }
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        url::form_urlencoded::byte_serialize(refresh.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(client_id.as_bytes()).collect::<String>(),
    );
    match ureq::post("https://accounts.spotify.com/api/token")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
    {
        Ok(resp) => {
            if let Ok(tk) = resp.into_json::<TokenResp>() {
                let mut st = state.lock().unwrap();
                st.access_token = tk.access_token;
                if !tk.refresh_token.is_empty() { st.refresh_token = tk.refresh_token; }
                st.token_expires_at = Some(Instant::now() + Duration::from_secs(tk.expires_in.saturating_sub(60) as u64));
            }
        }
        Err(e) => {
            eprintln!("[Spotify] Refresh failed: {}", e);
            state.lock().unwrap().connected = false;
        }
    }
}

#[derive(serde::Deserialize, Default)]
struct TokenResp {
    #[serde(default)] access_token: String,
    #[serde(default)] refresh_token: String,
    #[serde(default)] expires_in: i64,
}

fn load_client_id() -> Option<String> {
    let content = std::fs::read_to_string("spotify.properties").ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "spotify.client_id" {
                let v = v.trim();
                if !v.is_empty() && !v.starts_with("YOUR_") {
                    return Some(v.to_string());
                }
            }
        }
    }
    eprintln!("[Spotify] Set spotify.client_id in spotify.properties");
    None
}

fn rand_bytes(n: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut buf = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

fn base64_url(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

fn urlencoding_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", s).as_bytes())
        .next()
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| s.to_string())
}

use std::io::Read;
