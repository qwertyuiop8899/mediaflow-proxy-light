//! Vavoo extractor — resolves vavoo.to links via the Vavoo auth API.
//!
//! Two resolution paths are attempted, mirroring EasyProxy's logic:
//!   1. lokke.app `/api/app/ping` → `addonSig` → `mediahubmx-resolve.json`
//!      (returns a clean HLS URL).
//!   2. Fallback: `vavoo.tv/api/box/ping2` with the static `vec` payload →
//!      `response.signed` → built `https://www2.vavoo.to/live2/<token>.ts?...`
//!      URL with the signature passed via `vavoo_auth=`.
//!
//! Without the fallback, when lokke.app is rate-limited, geo-blocked or
//! returns no `addonSig`, extraction fails entirely.
use async_trait::async_trait;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::extractor::base::{
    BaseExtractor, ExtraParams, Extractor, ExtractorError, ExtractorResult,
};

const API_UA: &str = "okhttp/4.11.0";
const RESOLVE_UA: &str = "MediaHubMX/2";
const TS_FALLBACK_UA: &str = "VAVOO/2.6";
const AUTH_TOKEN: &str = "ldCvE092e7gER0rVIajfsXIvRhwlrAzP6_1oEJ4q6HH89QHt24v6NNL_jQJO219hiLOXF2hqEfsUuEWitEIGN4EaHHEHb7Cd7gojc5SQYRFzU3XWo_kMeryAUbcwWnQrnf0-";

const LOKKE_PING_URL: &str = "https://www.lokke.app/api/app/ping";
const RESOLVE_URL: &str = "https://vavoo.to/mediahubmx-resolve.json";
const TS_PING2_URL: &str = "https://www.vavoo.tv/api/box/ping2";

/// Static `vec` payload required by `ping2`.  Identical to EasyProxy /
/// plugin.video.vavooto — server validates this exact value.
const TS_VEC: &str = "9frjpxPjxSNilxJPCJ0XGYs6scej3dW/h/VWlnKUiLSG8IP7mfyDU7NirOlld+VtCKGj03XjetfliDMhIev7wcARo+YTU8KPFuVQP9E2DVXzY2BFo1NhE6qEmPfNDnm74eyl/7iFJ0EETm6XbYyz8IKBkAqPN/Spp3PZ2ulKg3QBSDxcVN4R5zRn7OsgLJ2CNTuWkd/h451lDCp+TtTuvnAEhcQckdsydFhTZCK5IiWrrTIC/d4qDXEd+GtOP4hPdoIuCaNzYfX3lLCwFENC6RZoTBYLrcKVVgbqyQZ7DnLqfLqvf3z0FVUWx9H21liGFpByzdnoxyFkue3NzrFtkRL37xkx9ITucepSYKzUVEfyBh+/3mtzKY26VIRkJFkpf8KVcCRNrTRQn47Wuq4gC7sSwT7eHCAydKSACcUMMdpPSvbvfOmIqeBNA83osX8FPFYUMZsjvYNEE3arbFiGsQlggBKgg1V3oN+5ni3Vjc5InHg/xv476LHDFnNdAJx448ph3DoAiJjr2g4ZTNynfSxdzA68qSuJY8UjyzgDjG0RIMv2h7DlQNjkAXv4k1BrPpfOiOqH67yIarNmkPIwrIV+W9TTV/yRyE1LEgOr4DK8uW2AUtHOPA2gn6P5sgFyi68w55MZBPepddfYTQ+E1N6R/hWnMYPt/i0xSUeMPekX47iucfpFBEv9Uh9zdGiEB+0P3LVMP+q+pbBU4o1NkKyY1V8wH1Wilr0a+q87kEnQ1LWYMMBhaP9yFseGSbYwdeLsX9uR1uPaN+u4woO2g8sw9Y5ze5XMgOVpFCZaut02I5k0U4WPyN5adQjG8sAzxsI3KsV04DEVymj224iqg2Lzz53Xz9yEy+7/85ILQpJ6llCyqpHLFyHq/kJxYPhDUF755WaHJEaFRPxUqbparNX+mCE9Xzy7Q/KTgAPiRS41FHXXv+7XSPp4cy9jli0BVnYf13Xsp28OGs/D8Nl3NgEn3/eUcMN80JRdsOrV62fnBVMBNf36+LbISdvsFAFr0xyuPGmlIETcFyxJkrGZnhHAxwzsvZ+Uwf8lffBfZFPRrNv+tgeeLpatVcHLHZGeTgWWml6tIHwWUqv2TVJeMkAEL5PPS4Gtbscau5HM+FEjtGS+KClfX1CNKvgYJl7mLDEf5ZYQv5kHaoQ6RcPaR6vUNn02zpq5/X3EPIgUKF0r/0ctmoT84B2J1BKfCbctdFY9br7JSJ6DvUxyde68jB+Il6qNcQwTFj4cNErk4x719Y42NoAnnQYC2/qfL/gAhJl8TKMvBt3Bno+va8ve8E0z8yEuMLUqe8OXLce6nCa+L5LYK1aBdb60BYbMeWk1qmG6Nk9OnYLhzDyrd9iHDd7X95OM6X5wiMVZRn5ebw4askTTc50xmrg4eic2U1w1JpSEjdH/u/hXrWKSMWAxaj34uQnMuWxPZEXoVxzGyuUbroXRfkhzpqmqqqOcypjsWPdq5BOUGL/Riwjm6yMI0x9kbO8+VoQ6RYfjAbxNriZ1cQ+AW1fqEgnRWXmjt4Z1M0ygUBi8w71bDML1YG6UHeC2cJ2CCCxSrfycKQhpSdI1QIuwd2eyIpd4LgwrMiY3xNWreAF+qobNxvE7ypKTISNrz0iYIhU0aKNlcGwYd0FXIRfKVBzSBe4MRK2pGLDNO6ytoHxvJweZ8h1XG8RWc4aB5gTnB7Tjiqym4b64lRdj1DPHJnzD4aqRixpXhzYzWVDN2kONCR5i2quYbnVFN4sSfLiKeOwKX4JdmzpYixNZXjLkG14seS6KR0Wl8Itp5IMIWFpnNokjRH76RYRZAcx0jP0V5/GfNNTi5QsEU98en0SiXHQGXnROiHpRUDXTl8FmJORjwXc0AjrEMuQ2FDJDmAIlKUSLhjbIiKw3iaqp5TVyXuz0ZMYBhnqhcwqULqtFSuIKpaW8FgF8QJfP2frADf4kKZG1bQ99MrRrb2A=";

pub struct VavooExtractor(pub BaseExtractor);

impl VavooExtractor {
    pub fn new(request_headers: HashMap<String, String>, proxy_url: Option<String>) -> Self {
        Self(BaseExtractor::new(request_headers, proxy_url))
    }

    /// Try the lokke.app + mediahubmx flow.  Returns `None` (instead of `Err`)
    /// when any step fails so the caller can fall back to ping2.
    async fn try_lokke_resolve(&self, url: &str) -> Option<String> {
        let unique_id = &Uuid::new_v4().to_string().replace('-', "")[..16];
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;

        let ping_body = serde_json::json!({
            "token": AUTH_TOKEN,
            "reason": "app-blur",
            "locale": "de",
            "theme": "dark",
            "metadata": {
                "device": {
                    "type": "Handset",
                    "brand": "google",
                    "model": "Nexus",
                    "name": "21081111RG",
                    "uniqueId": unique_id
                },
                "os": { "name": "android", "version": "7.1.2", "abis": ["arm64-v8a"], "host": "android" },
                "app": {
                    "platform": "android",
                    "version": "1.1.0",
                    "buildId": "97215000",
                    "engine": "hbc85",
                    "signatures": ["6e8a975e3cbf07d5de823a760d4c2547f86c1403105020adee5de67ac510999e"],
                    "installer": "com.android.vending"
                },
                "version": { "package": "app.lokke.main", "binary": "1.1.0", "js": "1.1.0" },
                "platform": {
                    "isAndroid": true,
                    "isIOS": false,
                    "isTV": false,
                    "isWeb": false,
                    "isMobile": true,
                    "isWebTV": false,
                    "isElectron": false
                }
            },
            "appFocusTime": 0,
            "playerActive": false,
            "playDuration": 0,
            "devMode": true,
            "hasAddon": true,
            "castConnected": false,
            "package": "app.lokke.main",
            "version": "1.1.0",
            "process": "app",
            "firstAppStart": now_ms - 86400000u64,
            "lastAppStart": now_ms,
            "ipLocation": null,
            "adblockEnabled": false,
            "proxy": {
                "supported": ["ss", "openvpn"],
                "engine": "openvpn",
                "ssVersion": 1,
                "enabled": false,
                "autoServer": true,
                "id": "fi-hel"
            },
            "iap": { "supported": true }
        });

        let auth_resp = self
            .0
            .client
            .post(LOKKE_PING_URL)
            .header("user-agent", API_UA)
            .header("accept", "application/json")
            .header("content-type", "application/json; charset=utf-8")
            .json(&ping_body)
            .send()
            .await
            .ok()?;

        if !auth_resp.status().is_success() {
            return None;
        }

        let auth_data: serde_json::Value = auth_resp.json().await.ok()?;
        let signature = auth_data["addonSig"].as_str()?;

        let resolve_body = serde_json::json!({
            "language": "de",
            "region": "AT",
            "url": url,
            "clientVersion": "3.0.2",
        });

        let resolve_resp = self
            .0
            .client
            .post(RESOLVE_URL)
            .header("user-agent", RESOLVE_UA)
            .header("accept", "application/json")
            .header("content-type", "application/json; charset=utf-8")
            .header("mediahubmx-signature", signature)
            .json(&resolve_body)
            .send()
            .await
            .ok()?;

        if !resolve_resp.status().is_success() {
            return None;
        }

        let resolve_data: serde_json::Value = resolve_resp.json().await.ok()?;

        if let Some(arr) = resolve_data.as_array() {
            arr.first()
                .and_then(|v| v["url"].as_str())
                .map(String::from)
        } else {
            resolve_data["url"]
                .as_str()
                .or_else(|| resolve_data["data"]["url"].as_str())
                .map(String::from)
        }
    }

    /// Fallback: get TS signature via ping2 and build a `live2/<token>.ts` URL.
    async fn try_ts_fallback(&self, url: &str) -> Option<String> {
        let resp = self
            .0
            .client
            .post(TS_PING2_URL)
            .header("content-type", "application/x-www-form-urlencoded")
            .form(&[("vec", TS_VEC)])
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let data: serde_json::Value = resp.json().await.ok()?;
        let signed = data["response"]["signed"].as_str()?;

        // Extract token from `/play/<token>` (terminated by `/`, `?`, `#` or end).
        let after_play = url.split("/play/").nth(1)?;
        let token: String = after_play
            .chars()
            .take_while(|c| !matches!(c, '/' | '?' | '#'))
            .collect();
        if token.is_empty() {
            return None;
        }

        let auth = urlencoding::encode(signed);
        Some(format!(
            "https://www2.vavoo.to/live2/{token}.ts?n=1&b=5&vavoo_auth={auth}"
        ))
    }
}

#[async_trait]
impl Extractor for VavooExtractor {
    fn host_name(&self) -> &'static str {
        "Vavoo"
    }

    async fn extract(
        &self,
        url: &str,
        _extra: &ExtraParams,
    ) -> Result<ExtractorResult, ExtractorError> {
        // Step 1: lokke.app + mediahubmx (preferred — returns clean HLS).
        let mut headers = HashMap::new();
        let final_url = if let Some(u) = self.try_lokke_resolve(url).await {
            headers.insert(
                "User-Agent".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            );
            headers.insert("Referer".to_string(), "https://vavoo.to".to_string());
            headers.insert("Origin".to_string(), "https://vavoo.to".to_string());
            u
        } else if let Some(u) = self.try_ts_fallback(url).await {
            // Step 2: TS signature fallback (works when lokke.app is unavailable).
            headers.insert("User-Agent".to_string(), TS_FALLBACK_UA.to_string());
            headers.insert("Referer".to_string(), "https://vavoo.to/".to_string());
            u
        } else {
            return Err(ExtractorError::extract(
                "Vavoo: both lokke.app and ping2 fallback failed to resolve the URL",
            ));
        };

        headers.insert("X-EasyProxy-Disable-SSL".to_string(), "1".to_string());

        // HLS manifests need the manifest proxy (segment URL rewriting); raw
        // `.ts` streams go through proxy_stream_endpoint.
        let endpoint = {
            let lower = final_url.to_lowercase();
            if lower.contains(".m3u8") || lower.contains(".m3u") || lower.contains(".m3u_plus") {
                "hls_manifest_proxy"
            } else {
                "proxy_stream_endpoint"
            }
        };

        Ok(ExtractorResult {
            destination_url: final_url,
            request_headers: headers,
            mediaflow_endpoint: endpoint,
        })
    }
}
