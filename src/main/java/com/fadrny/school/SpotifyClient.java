package com.fadrny.school;

import com.sun.net.httpserver.HttpServer;
import se.michaelthelin.spotify.SpotifyApi;
import se.michaelthelin.spotify.SpotifyHttpManager;
import se.michaelthelin.spotify.model_objects.credentials.AuthorizationCodeCredentials;
import se.michaelthelin.spotify.model_objects.miscellaneous.CurrentlyPlaying;
import se.michaelthelin.spotify.model_objects.specification.ArtistSimplified;
import se.michaelthelin.spotify.model_objects.specification.Image;
import se.michaelthelin.spotify.model_objects.specification.Track;

import java.awt.Desktop;
import java.io.*;
import java.net.InetSocketAddress;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.Base64;
import java.util.Properties;
import java.util.concurrent.*;

/**
 * Spotify integration – OAuth PKCE + currently-playing polling.
 *
 * @author Marek Fadrný
 */
public class SpotifyClient {

    private static final URI REDIRECT_URI = SpotifyHttpManager.makeUri("http://127.0.0.1:8888/callback");
    private static final int POLL_MS = 3000;

    private SpotifyApi api;
    private String codeVerifier;
    private long tokenExpiresAt;

    private volatile String trackName = "", artistName = "";
    private volatile byte[] pendingAlbumArt = null;
    private volatile boolean connected = false, authInProgress = false;
    private String lastArtUrl = "";

    private HttpServer callbackServer;
    private ScheduledExecutorService poller;
    private final HttpClient http = HttpClient.newHttpClient();
    private final CompletableFuture<String> authCodeFuture = new CompletableFuture<>();

    public SpotifyClient() {
        String clientId = loadProp("spotify.client_id");
        if (clientId != null) {
            api = new SpotifyApi.Builder().setClientId(clientId).setRedirectUri(REDIRECT_URI).build();
        }
    }

    private String loadProp(String key) {
        try (FileInputStream fis = new FileInputStream("spotify.properties")) {
            Properties p = new Properties();
            p.load(fis);
            String v = p.getProperty(key, "").trim();
            if (!v.isEmpty() && !v.startsWith("YOUR_"))
                return v;
        } catch (IOException ignored) {
        }
        System.err.println("[Spotify] Set " + key + " in spotify.properties");
        return null;
    }

    // Auth

    public void startAuth() {
        if (api == null || authInProgress)
            return;
        authInProgress = true;
        try {
            codeVerifier = base64Url(randomBytes(64));
            String challenge = base64Url(MessageDigest.getInstance("SHA-256")
                    .digest(codeVerifier.getBytes(StandardCharsets.UTF_8)));

            startCallbackServer();
            URI uri = api.authorizationCodePKCEUri(challenge)
                    .scope("user-read-currently-playing user-read-playback-state").build().execute();

            if (Desktop.isDesktopSupported())
                Desktop.getDesktop().browse(uri);
            else
                System.out.println("[Spotify] Open: " + uri);

            CompletableFuture.runAsync(() -> {
                try {
                    String code = authCodeFuture.get(120, TimeUnit.SECONDS);
                    AuthorizationCodeCredentials c = api.authorizationCodePKCE(code, codeVerifier).build().execute();
                    api.setAccessToken(c.getAccessToken());
                    api.setRefreshToken(c.getRefreshToken());
                    tokenExpiresAt = System.currentTimeMillis() + (c.getExpiresIn() - 60) * 1000L;
                    connected = true;
                    System.out.println("[Spotify] Authenticated!");
                    startPolling();
                } catch (Exception e) {
                    System.err.println("[Spotify] Auth failed: " + e.getMessage());
                } finally {
                    stopCallbackServer();
                    authInProgress = false;
                }
            });
        } catch (Exception e) {
            System.err.println("[Spotify] " + e.getMessage());
            authInProgress = false;
        }
    }

    private void startCallbackServer() throws IOException {
        callbackServer = HttpServer.create(new InetSocketAddress("127.0.0.1", 8888), 0);
        callbackServer.createContext("/callback", ex -> {
            String q = ex.getRequestURI().getQuery(), code = null;
            if (q != null)
                for (String p : q.split("&")) {
                    String[] kv = p.split("=", 2);
                    if (kv.length == 2 && kv[0].equals("code"))
                        code = kv[1];
                }
            String html = code != null
                    ? "<h1 style='color:#1DB954'>&#10003; Connected!</h1><p>Return to visualizer.</p>"
                    : "<h1 style='color:#f44'>Failed</h1>";
            byte[] b = html.getBytes();
            ex.getResponseHeaders().set("Content-Type", "text/html");
            ex.sendResponseHeaders(200, b.length);
            ex.getResponseBody().write(b);
            ex.getResponseBody().close();
            if (code != null)
                authCodeFuture.complete(code);
            else
                authCodeFuture.completeExceptionally(new RuntimeException("no code"));
        });
        callbackServer.setExecutor(Executors.newSingleThreadExecutor(r -> {
            Thread t = new Thread(r);
            t.setDaemon(true);
            return t;
        }));
        callbackServer.start();
    }

    private void stopCallbackServer() {
        if (callbackServer != null) {
            callbackServer.stop(0);
            callbackServer = null;
        }
    }

    private void refreshToken() {
        try {
            AuthorizationCodeCredentials c = api.authorizationCodePKCERefresh().build().execute();
            api.setAccessToken(c.getAccessToken());
            api.setRefreshToken(c.getRefreshToken());
            tokenExpiresAt = System.currentTimeMillis() + (c.getExpiresIn() - 60) * 1000L;
        } catch (Exception e) {
            System.err.println("[Spotify] Refresh failed: " + e.getMessage());
            connected = false;
        }
    }

    // Polling

    private void startPolling() {
        if (poller != null)
            return;
        poller = Executors.newSingleThreadScheduledExecutor(r -> {
            Thread t = new Thread(r, "Spotify-Poll");
            t.setDaemon(true);
            return t;
        });
        poller.scheduleWithFixedDelay(this::poll, 0, POLL_MS, TimeUnit.MILLISECONDS);
    }

    private void poll() {
        try {
            if (System.currentTimeMillis() >= tokenExpiresAt)
                refreshToken();
            if (!connected)
                return;

            CurrentlyPlaying cp = api.getUsersCurrentlyPlayingTrack().build().execute();
            if (cp == null || cp.getItem() == null || !(cp.getItem() instanceof Track track)) {
                trackName = "";
                artistName = "";
                return;
            }

            trackName = track.getName() != null ? track.getName() : "";
            ArtistSimplified[] artists = track.getArtists();
            artistName = artists != null && artists.length > 0
                    ? String.join(", ", java.util.Arrays.stream(artists).map(ArtistSimplified::getName).toList())
                    : "";

            if (track.getAlbum() != null && track.getAlbum().getImages() != null) {
                Image best = null;
                int bestDiff = Integer.MAX_VALUE;
                for (Image img : track.getAlbum().getImages()) {
                    int d = Math.abs((img.getWidth() != null ? img.getWidth() : 0) - 300);
                    if (d < bestDiff) {
                        bestDiff = d;
                        best = img;
                    }
                }
                if (best != null && !best.getUrl().equals(lastArtUrl)) {
                    lastArtUrl = best.getUrl();
                    HttpResponse<byte[]> r = http.send(HttpRequest.newBuilder(URI.create(lastArtUrl)).build(),
                            HttpResponse.BodyHandlers.ofByteArray());
                    if (r.statusCode() == 200)
                        pendingAlbumArt = r.body();
                }
            }
        } catch (Exception e) {
            System.err.println("[Spotify] Poll: " + e.getMessage());
        }
    }

    // Public API

    public String getTrackName() {
        return trackName;
    }

    public String getArtistName() {
        return artistName;
    }

    public boolean isConnected() {
        return connected;
    }

    public boolean isAuthInProgress() {
        return authInProgress;
    }

    public boolean hasTrack() {
        return !trackName.isEmpty();
    }

    public byte[] consumeNewAlbumArt() {
        byte[] a = pendingAlbumArt;
        if (a != null)
            pendingAlbumArt = null;
        return a;
    }

    public void stop() {
        if (poller != null) {
            poller.shutdownNow();
            poller = null;
        }
        stopCallbackServer();
        connected = false;
    }

    // Utils

    private static byte[] randomBytes(int n) {
        byte[] b = new byte[n];
        new SecureRandom().nextBytes(b);
        return b;
    }

    private static String base64Url(byte[] b) {
        return Base64.getUrlEncoder().withoutPadding().encodeToString(b);
    }
}
