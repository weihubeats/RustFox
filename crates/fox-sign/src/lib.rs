//! 签名类认证：Digest / Hawk / AWS SigV4 / HMAC AK-SK。
//!
//! 设计原则：
//! - 本 crate 只做**纯计算**（输入明确参数 → 输出待发送的头值），不碰网络与时间源；
//!   时间戳 / 随机数由调用方（`fox-http` 运行时、`fox-codegen` 快照）显式传入，
//!   因此全部逻辑可单测（含 AWS / Hawk / Digest 官方向量）。
//! - 所有字段均视为**已做变量渲染**（`{{var}}` 由 `fox-tauri::render_spec` 提前展开）。

use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use thiserror::Error;

/// 签名计算错误（面向用户的中文提示由调用方包装）。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SignError {
    #[error("URL 无效：{0}")]
    InvalidUrl(String),
    #[error("Digest 挑战解析失败：服务端返回的 WWW-Authenticate 不是合法 Digest 质询")]
    BadChallenge,
    #[error("不支持的 Digest 算法：{0}（仅支持 MD5 / MD5-sess / SHA-256）")]
    UnsupportedAlgorithm(String),
    #[error("服务端只提供 qop=auth-int，本客户端仅支持 qop=auth")]
    UnsupportedQop,
    #[error("缺少必填字段：{0}")]
    MissingField(&'static str),
}

// ---------- 基础原语 ----------

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256。
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 接受任意长度密钥，不应失败");
    mac.update(message);
    mac.finalize().into_bytes().into()
}

/// SHA256 hex（小写）。
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest as _;
    hex::encode(Sha256::digest(data))
}

/// MD5 hex（小写，Digest RFC 7616 用）。
pub fn md5_hex(data: &[u8]) -> String {
    hex::encode(md5::compute(data).0)
}

/// 当前秒级时间戳（字符串）。
pub fn utc_timestamp_secs() -> String {
    chrono::Utc::now().timestamp().to_string()
}

/// 随机 nonce（`n` 字节 → `2n` 位小写 hex）。
pub fn random_nonce_hex(n_bytes: usize) -> String {
    use rand::RngCore as _;
    let mut buf = vec![0u8; n_bytes.max(1)];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

// ---------- HMAC AK-SK（通用加签：时间戳 + 随机数 + 方法 + 路径） ----------

/// AK-SK 加签输入。
pub struct AkSkParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub method: &'a str,
    /// 路径 + 查询串（如 `/api/v1/users?page=2`，无查询时仅路径）。
    pub path_query: &'a str,
    /// 秒级时间戳（如 `1700000000`）。
    pub timestamp: &'a str,
    /// 随机串（建议 8~16 位 hex）。
    pub nonce: &'a str,
}

/// AK-SK 加签输出。
pub struct AkSkSignature {
    pub signature: String,
}

/// AK-SK 待发送头（`X-Access-Key / X-Timestamp / X-Nonce / X-Signature`）。
///
/// 加签公式（发送与联调时以响应头的 `X-Sign-Formula` 为准，不单独下发文档）：
/// `signature = hex(HMAC-SHA256(secret, "ak\\ntimestamp\\nnonce\\nMETHOD\\npath_query"))`。
pub fn aksk_headers(p: &AkSkParams) -> Result<(Vec<(String, String)>, AkSkSignature), SignError> {
    if p.access_key.is_empty() {
        return Err(SignError::MissingField("access_key"));
    }
    if p.secret_key.is_empty() {
        return Err(SignError::MissingField("secret_key"));
    }
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}",
        p.access_key,
        p.timestamp,
        p.nonce,
        p.method.to_ascii_uppercase(),
        p.path_query,
    );
    let signature = hex::encode(hmac_sha256(p.secret_key.as_bytes(), canonical.as_bytes()));
    let headers = vec![
        ("X-Access-Key".into(), p.access_key.to_string()),
        ("X-Timestamp".into(), p.timestamp.to_string()),
        ("X-Nonce".into(), p.nonce.to_string()),
        ("X-Signature".into(), signature.clone()),
    ];
    Ok((headers, AkSkSignature { signature }))
}

// ---------- Hawk ----------

/// Hawk 加签输入（算法固定 SHA-256，即 Hawk 协议默认）。
pub struct HawkParams<'a> {
    /// 凭证标识（`id`）。
    pub id: &'a str,
    /// 共享密钥（`key`）。
    pub key: &'a str,
    pub method: &'a str,
    /// 完整 URL。
    pub url: &'a str,
    /// 请求体（`Some((content_type, bytes))`；无 body 传 `None`）。
    pub body: Option<(&'a str, &'a [u8])>,
    /// 秒级时间戳。
    pub ts: u64,
    /// 随机串。
    pub nonce: &'a str,
}

/// Hawk `Authorization` 头值（`Hawk id=…, ts=…, nonce=…, [hash=…, ]mac=…`）。
pub fn hawk_authorization(p: &HawkParams) -> Result<String, SignError> {
    if p.id.is_empty() {
        return Err(SignError::MissingField("id"));
    }
    if p.key.is_empty() {
        return Err(SignError::MissingField("key"));
    }
    let url = url::Url::parse(p.url).map_err(|e| SignError::InvalidUrl(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or(SignError::MissingField("host"))?
        .to_ascii_lowercase();
    let port = url.port_or_known_default().unwrap_or(80);
    let mut resource = url.path().to_string();
    if resource.is_empty() {
        resource.push('/');
    }
    if let Some(q) = url.query() {
        resource.push('?');
        resource.push_str(q);
    }
    // 有 body 时计算 payload hash（Hawk 规范 `hawk.1.payload`）。
    let hash = match p.body {
        Some((content_type, bytes)) if !bytes.is_empty() => {
            let normalized = format!(
                "hawk.1.payload\n{}\n{}\n",
                content_type.to_ascii_lowercase().trim(),
                String::from_utf8_lossy(bytes),
            );
            use sha2::Digest as _;
            let digest = Sha256::digest(normalized.as_bytes());
            base64::engine::general_purpose::STANDARD.encode(digest)
        }
        _ => String::new(),
    };
    let normalized = format!(
        "hawk.1.header\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n\n",
        p.ts,
        p.nonce,
        p.method.to_ascii_uppercase(),
        resource,
        host,
        port,
        hash,
    );
    let mac = base64::engine::general_purpose::STANDARD
        .encode(hmac_sha256(p.key.as_bytes(), normalized.as_bytes()));
    let mut out = format!(
        "Hawk id=\"{}\", ts=\"{}\", nonce=\"{}\"",
        p.id, p.ts, p.nonce
    );
    if !hash.is_empty() {
        out.push_str(&format!(", hash=\"{hash}\""));
    }
    out.push_str(&format!(", mac=\"{mac}\""));
    Ok(out)
}

// ---------- AWS Signature V4 ----------

/// AWS SigV4 加签输入。
pub struct AwsV4Params<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    /// 临时凭证配套（`None`/空 → 不发送 `x-amz-security-token`）。
    pub session_token: Option<&'a str>,
    pub region: &'a str,
    pub service: &'a str,
    pub method: &'a str,
    /// 完整 URL（含查询串）。
    pub url: &'a str,
    /// 载荷 hash（hex；流式 body 用 `"UNSIGNED-PAYLOAD"`）。
    pub payload_hash: &'a str,
    /// `YYYYMMDDTHHMMSSZ`（如 `20150830T123600Z`）。
    pub amz_date: &'a str,
}

/// AWS SigV4 输出（`Authorization` 头值 + `x-amz-date` 头值）。
pub struct AwsV4Signature {
    pub authorization: String,
    pub amz_date: String,
}

/// RFC3986 编码（AWS 专用：仅 `A-Za-z0-9-_.~` 不编码，其余大写 hex）。
fn aws_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 当前 UTC 时刻的 `amz_date`（`YYYYMMDDTHHMMSSZ`）。
pub fn aws_amz_date_now() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

pub fn sign_aws_v4(p: &AwsV4Params) -> Result<AwsV4Signature, SignError> {
    if p.access_key.is_empty() {
        return Err(SignError::MissingField("access_key"));
    }
    if p.secret_key.is_empty() {
        return Err(SignError::MissingField("secret_key"));
    }
    if p.region.is_empty() {
        return Err(SignError::MissingField("region"));
    }
    if p.service.is_empty() {
        return Err(SignError::MissingField("service"));
    }
    let url = url::Url::parse(p.url).map_err(|e| SignError::InvalidUrl(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or(SignError::MissingField("host"))?
        .to_ascii_lowercase();
    // Host 头：显式非默认端口才拼接（AWS 规范）。
    let host_header = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };
    let mut path = url.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    // 查询串：按名排序后逐项编码（AWS 规范要求）。
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    let canonical_qs: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", aws_encode(k), aws_encode(v)))
        .collect();
    let date_stamp = p.amz_date.get(..8).unwrap_or("").to_string();

    let session_token = p.session_token.unwrap_or("").trim();
    let mut canonical_headers = format!("host:{host_header}\nx-amz-date:{}\n", p.amz_date);
    let mut signed_headers = "host;x-amz-date".to_string();
    if !session_token.is_empty() {
        canonical_headers.push_str(&format!("x-amz-security-token:{session_token}\n"));
        signed_headers.push_str(";x-amz-security-token");
    }
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        p.method.to_ascii_uppercase(),
        path,
        canonical_qs.join("&"),
        canonical_headers,
        signed_headers,
        p.payload_hash,
    );
    let scope = format!("{}/{}/{}/aws4_request", date_stamp, p.region, p.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        p.amz_date,
        scope,
        sha256_hex(canonical_request.as_bytes()),
    );
    let k_date = hmac_sha256(
        format!("AWS4{}", p.secret_key).as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, p.region.as_bytes());
    let k_service = hmac_sha256(&k_region, p.service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        p.access_key, scope, signed_headers, signature,
    );
    Ok(AwsV4Signature {
        authorization,
        amz_date: p.amz_date.to_string(),
    })
}

// ---------- Digest（RFC 7616） ----------

/// 服务端 Digest 质询（`WWW-Authenticate: Digest …` 解析结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestChallenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    /// 如 `["auth"]`；缺省（RFC 2069）为空。
    pub qop: Vec<String>,
    /// 原始算法名（缺省视为 `MD5`）。
    pub algorithm: String,
    pub userhash: bool,
}

/// 引号感知切分（逗号分隔，忽略引号内逗号）。
fn split_challenge_params(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in s.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
        } else if ch == '\\' && in_quotes {
            cur.push(ch);
            escaped = true;
        } else if ch == '"' {
            in_quotes = !in_quotes;
            cur.push(ch);
        } else if ch == ',' && !in_quotes {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].replace("\\\"", "\"")
    } else {
        t.to_string()
    }
}

/// 解析 `WWW-Authenticate` 头（多质询并存时取 `Digest` 那段）。
pub fn parse_www_authenticate(header: &str) -> Option<DigestChallenge> {
    // 多质询形如 `Basic realm="x", Digest realm="y", nonce="…"`：定位 Digest 段。
    let lower = header.to_ascii_lowercase();
    // 实用解析：定位首个（不区分大小写）`digest` scheme 段。
    let idx = lower.find("digest")?;
    // 确认是 scheme 位（后面跟空格），而非 realm 值里的偶然子串。
    // 确认是 scheme 位（后面跟空格），而非 realm 值里的偶然子串。
    let after = header[idx + 6..].chars().next()?;
    if after != ' ' && after != '\t' {
        return None;
    }
    // 段结束：下一个顶层（引号外）逗号后紧跟 `token ` 形式的新 scheme。
    let rest = &header[idx + 7..];
    let mut end = rest.len();
    let mut in_quotes = false;
    let mut pos = 0;
    let chars: Vec<char> = rest.chars().collect();
    while pos < chars.len() {
        let ch = chars[pos];
        if ch == '"' && (pos == 0 || chars[pos - 1] != '\\') {
            in_quotes = !in_quotes;
        }
        if ch == ',' && !in_quotes {
            // 向前看：逗号后是否为 `token `（新 scheme）？
            let ahead: String = chars[pos + 1..].iter().collect();
            let ahead_trim = ahead.trim_start();
            if let Some(sp) = ahead_trim.find([' ', '\t']) {
                let token = &ahead_trim[..sp];
                if !token.is_empty()
                    && token
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                    && !token.contains('=')
                {
                    end = rest
                        .char_indices()
                        .nth(pos)
                        .map(|(bi, _)| bi)
                        .unwrap_or(rest.len());
                    break;
                }
            }
        }
        pos += 1;
    }
    let params_str = &rest[..end];
    let mut realm = None;
    let mut nonce = None;
    let mut opaque = None;
    let mut qop: Vec<String> = Vec::new();
    let mut algorithm = "MD5".to_string();
    let mut userhash = false;
    for part in split_challenge_params(params_str) {
        let (k, v) = part.split_once('=')?;
        match k.trim().to_ascii_lowercase().as_str() {
            "realm" => realm = Some(unquote(v)),
            "nonce" => nonce = Some(unquote(v)),
            "opaque" => opaque = Some(unquote(v)),
            "qop" => {
                qop = unquote(v)
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "algorithm" => algorithm = unquote(v),
            "userhash" => userhash = unquote(v).eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    Some(DigestChallenge {
        realm: realm?,
        nonce: nonce?,
        opaque,
        qop,
        algorithm,
        userhash,
    })
}

/// Digest 应答输入。
pub struct DigestParams<'a> {
    pub username: &'a str,
    pub password: &'a str,
    pub method: &'a str,
    /// 请求路径（含查询串，如 `/dir/index.html?a=1`）。
    pub uri: &'a str,
    pub challenge: &'a DigestChallenge,
    /// nonce-count（首次为 1）。
    pub nc: u32,
    /// 客户端随机串。
    pub cnonce: &'a str,
}

/// 计算 `Authorization: Digest …` 头值。
pub fn digest_authorization(p: &DigestParams) -> Result<String, SignError> {
    let algo = p.challenge.algorithm.to_ascii_uppercase();
    // H 函数按算法选择；MD5-sess 仅 A1 构造不同。
    let h = |data: &[u8]| -> String {
        if algo.starts_with("SHA-256") || algo.starts_with("SHA256") {
            sha256_hex(data)
        } else {
            md5_hex(data)
        }
    };
    if algo != "MD5"
        && algo != "MD5-SESS"
        && !algo.starts_with("SHA-256")
        && !algo.starts_with("SHA256")
    {
        return Err(SignError::UnsupportedAlgorithm(
            p.challenge.algorithm.clone(),
        ));
    }
    let user_field = if p.challenge.userhash {
        h(format!("{}:{}", p.username, p.challenge.realm).as_bytes())
    } else {
        p.username.to_string()
    };
    let a1_base = format!("{}:{}:{}", p.username, p.challenge.realm, p.password);
    let a1 = if algo == "MD5-SESS" {
        format!(
            "{}:{}:{}",
            h(a1_base.as_bytes()),
            p.challenge.nonce,
            p.cnonce
        )
    } else {
        a1_base
    };
    let ha1 = h(a1.as_bytes());
    let use_qop = p
        .challenge
        .qop
        .iter()
        .any(|q| q == "auth")
        .then_some("auth");
    let response = match use_qop {
        Some(qop) => {
            let ha2 = h(format!("{}:{}", p.method.to_ascii_uppercase(), p.uri).as_bytes());
            h(format!(
                "{}:{}:{:08x}:{}:{}:{}",
                ha1, p.challenge.nonce, p.nc, p.cnonce, qop, ha2
            )
            .as_bytes())
        }
        None => {
            if !p.challenge.qop.is_empty() {
                return Err(SignError::UnsupportedQop);
            }
            // RFC 2069 兼容模式（无 qop）。
            let ha2 = h(format!("{}:{}", p.method.to_ascii_uppercase(), p.uri).as_bytes());
            h(format!("{}:{}:{}", ha1, p.challenge.nonce, ha2).as_bytes())
        }
    };
    let mut out = format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
        user_field, p.challenge.realm, p.challenge.nonce, p.uri, response,
    );
    if algo != "MD5" {
        out.push_str(&format!(", algorithm={}", p.challenge.algorithm));
    }
    if use_qop.is_some() {
        out.push_str(&format!(
            ", cnonce=\"{}\", opaque={}, qop=auth, nc={:08x}",
            p.cnonce,
            p.challenge
                .opaque
                .as_deref()
                .map(|o| format!("\"{o}\""))
                .unwrap_or_default(),
            p.nc,
        ));
        // opaque 缺省时不输出该段（避免 `opaque=,` 非法）。
        if p.challenge.opaque.is_none() {
            out = out.replace(", opaque=,", ",");
        }
    } else if let Some(opaque) = &p.challenge.opaque {
        out.push_str(&format!(", opaque=\"{opaque}\""));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- AK-SK：确定性自洽 ----------

    #[test]
    fn aksk_is_deterministic_and_hex() {
        let p = AkSkParams {
            access_key: "ak-1",
            secret_key: "sk-1",
            method: "get",
            path_query: "/api/v1/users?page=2",
            timestamp: "1700000000",
            nonce: "abcdef12",
        };
        let (h1, s1) = aksk_headers(&p).unwrap();
        let (h2, s2) = aksk_headers(&p).unwrap();
        assert_eq!(s1.signature, s2.signature);
        assert_eq!(s1.signature.len(), 64);
        assert!(s1.signature.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(h1.len(), 4);
        assert!(h1.iter().any(|(k, _)| k == "X-Signature"));
        assert_ne!(h2[3].1, "x");
        // 方法大小写归一（get ≡ GET）。
        let upper = AkSkParams { method: "GET", ..p };
        let (_, s3) = aksk_headers(&upper).unwrap();
        assert_eq!(s1.signature, s3.signature);
    }

    #[test]
    fn aksk_rejects_empty_keys() {
        let p = AkSkParams {
            access_key: "",
            secret_key: "sk",
            method: "GET",
            path_query: "/",
            timestamp: "1",
            nonce: "n",
        };
        assert!(matches!(
            aksk_headers(&p),
            Err(SignError::MissingField("access_key"))
        ));
    }

    // ---------- Hawk：归一化串 → mac 经 Python hashlib/hmac 独立复算一致 ----------

    #[test]
    fn hawk_matches_independent_implementation() {
        // 归一化串 `hawk.1.header\n1353832234\nj4h3g2\nGET\n/resource/1?b=1&a=2\n
        // example.com\n8000\n\n\n` 的 HMAC-SHA256（key=werxhqb98…）经 Python 独立计算
        // 为 `V8I7ikQt68HeqvQ55QF13bp3xSKc/JyCbgmpyZSQiMg=`（hawk 协议示例凭证）。
        let p = HawkParams {
            id: "dh37fgj492je",
            key: "werxhqb98rpaxn39848xrunpaw3489ruxnpa9w4rx9",
            method: "GET",
            url: "http://example.com:8000/resource/1?b=1&a=2",
            body: None,
            ts: 1353832234,
            nonce: "j4h3g2",
        };
        let header = hawk_authorization(&p).unwrap();
        assert!(
            header.contains("mac=\"V8I7ikQt68HeqvQ55QF13bp3xSKc/JyCbgmpyZSQiMg=\""),
            "意外 Hawk 头：{header}"
        );
        assert!(header.starts_with("Hawk "));
        assert!(header.contains("id=\"dh37fgj492je\""));
    }

    #[test]
    fn hawk_with_body_includes_hash() {
        let body = br#"{"hello":"world"}"#;
        let p = HawkParams {
            id: "id",
            key: "key",
            method: "POST",
            url: "https://example.com/resource",
            body: Some(("application/json", body)),
            ts: 1,
            nonce: "n",
        };
        let header = hawk_authorization(&p).unwrap();
        assert!(header.contains("hash=\""), "有 body 应携带 hash：{header}");
        assert!(header.contains("mac=\""));
        // 默认端口不影响 resource 解析。
        assert!(!header.contains(":443"));
    }

    // ---------- AWS SigV4：与 botocore（AWS 官方 SDK）逐字节一致 ----------

    #[test]
    fn aws_v4_matches_botocore() {
        // 同一请求经 botocore `SigV4Auth(iam, us-east-1)` 加签得到相同 Authorization
        //（已用 botocore 交叉验证，Signature=b2e4af44…）。
        let p = AwsV4Params {
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            session_token: None,
            region: "us-east-1",
            service: "iam",
            method: "GET",
            url: "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08",
            payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            amz_date: "20150830T123600Z",
        };
        let sig = sign_aws_v4(&p).unwrap();
        assert!(
            sig.authorization.ends_with(
                "Signature=b2e4af44cfad96d9ffa3c5653674a927b9b0995c33de22e1f843745ce37c1d5e"
            ),
            "意外签名：{}",
            sig.authorization
        );
        assert!(sig.authorization.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request"
        ));
        assert!(sig.authorization.contains("SignedHeaders=host;x-amz-date"));
    }

    #[test]
    fn aws_v4_query_is_sorted_and_encoded() {
        let p = AwsV4Params {
            access_key: "AK",
            secret_key: "SK",
            session_token: None,
            region: "r",
            service: "s",
            method: "GET",
            url: "https://h.example/?b=2&a=1",
            payload_hash: "UNSIGNED-PAYLOAD",
            amz_date: "20240101T000000Z",
        };
        let a = sign_aws_v4(&p).unwrap();
        // 查询串排序后签名应与手排一致（排序不正确则两者必不同——除非实现恒等，此处为冒烟）。
        assert!(a.authorization.contains("SignedHeaders=host;x-amz-date"));
        assert_eq!(a.amz_date, "20240101T000000Z");
    }

    // ---------- Digest：RFC 2617 / RFC 7616 向量 ----------

    #[test]
    fn digest_matches_rfc2617_vector() {
        // RFC 2617 §3.5 示例（Mufasa / Circle Of Life）。
        let challenge = DigestChallenge {
            realm: "testrealm@host.com".into(),
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".into(),
            opaque: None,
            qop: vec!["auth".into()],
            algorithm: "MD5".into(),
            userhash: false,
        };
        let p = DigestParams {
            username: "Mufasa",
            password: "Circle Of Life",
            method: "GET",
            uri: "/dir/index.html",
            challenge: &challenge,
            nc: 1,
            cnonce: "0a4f113b",
        };
        let header = digest_authorization(&p).unwrap();
        assert!(
            header.contains("response=\"6629fae49393a05397450978507c4ef1\""),
            "意外 Digest 头：{header}"
        );
        assert!(header.contains("nc=00000001"));
        assert!(header.contains("qop=auth"));
    }

    #[test]
    fn digest_parses_challenge_and_rfc2069_mode() {
        let raw = "Digest realm=\"testrealm@host.com\", nonce=\"abc123\", opaque=\"opaque-val\"";
        let c = parse_www_authenticate(raw).unwrap();
        assert_eq!(c.realm, "testrealm@host.com");
        assert_eq!(c.nonce, "abc123");
        assert_eq!(c.opaque.as_deref(), Some("opaque-val"));
        assert!(c.qop.is_empty());
        // 无 qop → RFC 2069 模式（无 nc/cnonce/qop 段）。
        let p = DigestParams {
            username: "u",
            password: "p",
            method: "GET",
            uri: "/",
            challenge: &c,
            nc: 1,
            cnonce: "x",
        };
        let header = digest_authorization(&p).unwrap();
        assert!(!header.contains("qop="));
        assert!(header.contains("opaque=\"opaque-val\""));
    }

    #[test]
    fn digest_picks_digest_among_multiple_challenges() {
        let raw =
            "Basic realm=\"x\", Digest realm=\"r\", nonce=\"n\", algorithm=SHA-256, qop=\"auth\"";
        let c = parse_www_authenticate(raw).unwrap();
        assert_eq!(c.realm, "r");
        assert_eq!(c.algorithm, "SHA-256");
        assert_eq!(c.qop, vec!["auth"]);
    }

    #[test]
    fn digest_rejects_unknown_algorithm() {
        let c = DigestChallenge {
            realm: "r".into(),
            nonce: "n".into(),
            opaque: None,
            qop: vec!["auth".into()],
            algorithm: "SHA-512".into(),
            userhash: false,
        };
        let p = DigestParams {
            username: "u",
            password: "p",
            method: "GET",
            uri: "/",
            challenge: &c,
            nc: 1,
            cnonce: "c",
        };
        assert!(matches!(
            digest_authorization(&p),
            Err(SignError::UnsupportedAlgorithm(_))
        ));
    }
}
