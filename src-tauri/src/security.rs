use std::net::{IpAddr, SocketAddr};

use encoding_rs::{Encoding, UTF_8};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use url::Url;

use crate::api::AppState;
use crate::error::{Result, message};

const MAX_REMOTE_BYTES: usize = 25 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug)]
pub struct RemoteText {
    pub text: String,
    pub final_url: Url,
}

pub async fn fetch_public_text(raw_url: &str, state: &AppState) -> Result<RemoteText> {
    let mut url = normalize_remote_url(raw_url)?;
    for _ in 0..=MAX_REDIRECTS {
        validate_public_target(&url).await?;
        let response = state.client.get(url.clone()).send().await?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| message("リダイレクト先URLがありません。"))?;
            url = normalize_remote_url(url.join(location)?.as_str())?;
            continue;
        }
        if !response.status().is_success() {
            return Err(message(format!(
                "HTTP {} を受信しました。",
                response.status().as_u16()
            )));
        }
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_REMOTE_BYTES)
        {
            return Err(message("取得データが大きすぎます。"));
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = response.bytes().await?;
        if bytes.len() > MAX_REMOTE_BYTES {
            return Err(message("取得データが大きすぎます。"));
        }
        return Ok(RemoteText {
            text: decode_text(&bytes, &content_type),
            final_url: url,
        });
    }
    Err(message("リダイレクト回数が多すぎます。"))
}

pub fn normalize_remote_url(value: &str) -> Result<Url> {
    let url = Url::parse(value.trim())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(message("http/https以外のURLは利用できません。"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(message("認証情報を含むURLは利用できません。"));
    }
    if url.host_str().is_none() {
        return Err(message("URLにホスト名がありません。"));
    }
    Ok(url)
}

async fn validate_public_target(url: &Url) -> Result<()> {
    let host = url
        .host_str()
        .ok_or_else(|| message("URLにホスト名がありません。"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| message("URLのポートが不正です。"))?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await?.collect();
    if addresses.is_empty() {
        return Err(message("URLのホスト名を解決できませんでした。"));
    }
    for address in addresses {
        ensure_public_ip(address.ip())?;
    }
    Ok(())
}

pub fn ensure_public_ip(address: IpAddr) -> Result<()> {
    let blocked = match address {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 224
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| ensure_public_ip(IpAddr::V4(mapped)).is_err())
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] & 0xffc0) == 0xfec0
                || segments[0] == 0x2001
                    && matches!(segments[1], 0x0000 | 0x0002 | 0x000d | 0x0010 | 0x0db8)
                || segments[0] == 0x2002
                || segments[0] == 0x0064 && segments[1] == 0xff9b
        }
    };
    if blocked {
        Err(message(
            "ローカルまたは予約済みネットワークへの接続は許可されていません。",
        ))
    } else {
        Ok(())
    }
}

fn decode_text(bytes: &[u8], content_type: &str) -> String {
    let charset = content_type
        .split(';')
        .find_map(|part| part.trim().strip_prefix("charset="))
        .map(str::trim)
        .map(|value| value.trim_matches(['"', '\'']).as_bytes().to_vec());
    let encoding = charset
        .as_deref()
        .and_then(Encoding::for_label)
        .unwrap_or(UTF_8);
    let (text, _, _) = encoding.decode(bytes);
    text.into_owned().replace('\0', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_mapped_addresses_are_rejected() {
        assert!(ensure_public_ip("127.0.0.1".parse().unwrap()).is_err());
        assert!(ensure_public_ip("::ffff:127.0.0.1".parse().unwrap()).is_err());
        assert!(ensure_public_ip("fc00::1".parse().unwrap()).is_err());
        assert!(ensure_public_ip("8.8.8.8".parse().unwrap()).is_ok());
    }
}
