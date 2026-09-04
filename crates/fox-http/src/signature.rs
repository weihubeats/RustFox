//! 动态签名鉴权（Dynamic Signature）。
//!
//! 与 OAuth2 等「登录后换取凭证」不同，动态签名是**无状态**的：服务端只凭
//! 请求头里的 Key / Timestamp / Sig 三要素即时校验，不存在存储的会话。
//! 因此 Sig 必须覆盖请求三要素，且 Timestamp 必须足够新，防止重放攻击——
//! 这也决定了签名计算必须发生在**发送前最后一刻**（Pre-Request Hook）：
//! 由本模块在 `send_request_inner` 内部同步完成，全程在 Rust 进程内，不经过
//! 任何 IPC 往返，时间戳即为本机当前毫秒时间，无前端→后端延迟。
//!
//! 扩展性设计：计算管线拆成「载荷渲染 → 摘要 → 编码」三层，彼此正交：
//!
//! - **载荷渲染**：模板替换（当前 `{{$key}}` / `{{$secret}}` / `{{$timestamp}}`），
//!   未来接入 AWS SigV4 等排序签名时，只需扩展模板标记或换用专门的
//!   canonical 请求拼装器（详见 `apply_signature` 注释）；
//! - **摘要**：MD5 / SHA256 / HMAC-SHA256 已支持，新增算法枚举加一个变体即可；
//! - **编码**：HexLower / HexUpper / Base64 已支持，同样枚举扩展。

use std::fmt::Write;

use base64::Engine;
use fox_core::model::{DynamicSignatureConfig, SignatureAlgorithm, SignatureEncoding};
use fox_core::AppError;

/// 模板内置占位符。
const PLACEHOLDER_KEY: &str = "{{$key}}";
const PLACEHOLDER_SECRET: &str = "{{$secret}}";
const PLACEHOLDER_TIMESTAMP: &str = "{{$timestamp}}";

/// 载荷模板渲染：一次性扫描替换全部占位符，避免多次 `str::replace` 的
/// 重复扫描（模板通常很短，但大量请求下仍有意义）。未知的 `{{...}}`
/// 原样保留，交由服务端容错，不视为错误。
fn render_payload(template: &str, key: &str, secret: &str, timestamp: i64) -> String {
    // 快路径：模板不含任何占位符时无需扫描，直接返回借用（无分配）。
    if !template.contains(PLACEHOLDER_KEY)
        && !template.contains(PLACEHOLDER_SECRET)
        && !template.contains(PLACEHOLDER_TIMESTAMP)
    {
        return template.to_string();
    }

    let mut out = String::with_capacity(template.len() + 64);
    let mut rest = template;
    loop {
        // 三个占位符取文本上最先出现者（索引最小），保证嵌套/重叠场景下
        // 从左到右的替换顺序与 `str::replace` 语义一致。
        let mut next = (usize::MAX, "", "");
        for ph in [PLACEHOLDER_KEY, PLACEHOLDER_SECRET, PLACEHOLDER_TIMESTAMP] {
            if let Some(idx) = rest.find(ph) {
                if idx < next.0 {
                    next = (idx, ph, ph);
                }
            }
        }
        let (idx, ph, _) = next;
        if idx == usize::MAX {
            break;
        }
        out.push_str(&rest[..idx]);
        match ph {
            // Key / Secret 已是借用，直接压入，零分配。
            PLACEHOLDER_KEY => out.push_str(key),
            PLACEHOLDER_SECRET => out.push_str(secret),
            // 时间戳为 i64，需转十进制；用栈缓冲避免临时 String 分配。
            PLACEHOLDER_TIMESTAMP => {
                let mut buf = String::new();
                let _ = write!(buf, "{timestamp}");
                out.push_str(&buf);
            }
            _ => unreachable!("占位符集合已固定"),
        }
        rest = &rest[idx + ph.len()..];
    }
    out.push_str(rest);
    out
}

/// 摘要：按算法把载荷变成固定长度字节。
fn digest(algorithm: SignatureAlgorithm, secret: &str, payload: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    match algorithm {
        // MD5：RFC 1321，16 字节摘要。
        SignatureAlgorithm::MD5 => {
            let mut h = md5::Md5::new();
            h.update(payload.as_bytes());
            h.finalize().to_vec()
        }
        SignatureAlgorithm::SHA256 => {
            let mut h = Sha256::new();
            h.update(payload.as_bytes());
            h.finalize().to_vec()
        }
        SignatureAlgorithm::HmacSHA256 => {
            use hmac::{Hmac, Mac};
            // RFC 2104：HMAC 密钥为 app_secret，载荷为被签消息。
            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
                .expect("HMAC 接受任意长度密钥，构造不会失败");
            mac.update(payload.as_bytes());
            mac.finalize().into_bytes().to_vec()
        }
    }
}

/// 摘要字节 → 传输字符串。
fn encode(encoding: SignatureEncoding, bytes: &[u8]) -> String {
    match encoding {
        SignatureEncoding::HexLower => {
            let mut out = String::with_capacity(bytes.len() * 2);
            for b in bytes {
                // fmt::Write 到 String 不会失败，忽略返回值。
                let _ = write!(out, "{b:02x}");
            }
            out
        }
        SignatureEncoding::HexUpper => {
            let mut out = String::with_capacity(bytes.len() * 2);
            for b in bytes {
                let _ = write!(out, "{b:02X}");
            }
            out
        }
        SignatureEncoding::Base64 => base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

/// 动态签名 Pre-Request Hook：把 Key / Timestamp / Sig 三个请求头注入 `headers`。
///
/// **为什么是 Pre-Request 而非请求前由前端预计算**：签名里时间戳若在前端
/// 生成，需经 IPC 传到后端再发出，延迟（毫秒级）直接浪费在服务端的时间窗
/// 校验上；这里在发送管线最末端计算，时间戳必然最新。
///
/// 对将来 AWS SigV4 之类需要 **字典序排序 + 参与计算 body 摘要** 的签名，
/// 只需新增一个渲染器：把 `payload_template` 换成按 header 名排序后的
/// canonical string（加 `{{$body_hash}}` 占位符），`digest` / `encode`
/// 两层完全复用，`apply_auth` 的调用点也不用动。
pub fn apply_signature(
    headers: &mut Vec<(String, String)>,
    config: &DynamicSignatureConfig,
) -> Result<(), AppError> {
    let key = config.app_key.trim();
    let secret = config.app_secret.trim();
    let timestamp = chrono::Utc::now().timestamp_millis();

    if key.is_empty() {
        return Err(AppError::Validation("动态签名：App-Key 不能为空".into()));
    }
    if secret.is_empty() {
        return Err(AppError::Validation("动态签名：App-Secret 不能为空".into()));
    }
    if config.payload_template.trim().is_empty() {
        return Err(AppError::Validation("动态签名：载荷模板不能为空".into()));
    }

    let payload = render_payload(&config.payload_template, key, secret, timestamp);
    let raw = digest(config.algorithm, secret, &payload);
    let sig = encode(config.encoding, &raw);

    // 用户自定义的 header 名可能覆盖默认值；同一头名重复推送时由 reqwest
    // 去重（HeaderMap 按名覆盖），与用户手动填写的同名头冲突时后者覆盖前者。
    headers.push((config.key_header.clone(), key.to_string()));
    headers.push((config.timestamp_header.clone(), timestamp.to_string()));
    headers.push((config.sig_header.clone(), sig));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DynamicSignatureConfig {
        DynamicSignatureConfig {
            app_key: "app-123".into(),
            app_secret: "sec-456".into(),
            ..Default::default()
        }
    }

    #[test]
    fn md5_hex_lower_matches_known_vector() {
        // 对照 `printf 'app-123sec-4561700000000000' | md5`（OpenSSL/BSD md5）。
        let c = cfg();
        let payload = render_payload(&c.payload_template, "app-123", "sec-456", 1_700_000_000_000);
        let raw = digest(c.algorithm, "sec-456", &payload);
        let sig = encode(c.encoding, &raw);
        assert_eq!(sig, "f2afd049faba22aad592d0d8dcb14543");
        // MD5 全长 32 位 hex。
        assert_eq!(sig.len(), 32);
    }

    #[test]
    fn md5_hex_upper_and_base64() {
        let c = cfg();
        let payload = render_payload(&c.payload_template, "app-123", "sec-456", 1_700_000_000_000);
        let raw = digest(c.algorithm, "sec-456", &payload);
        assert_eq!(
            encode(SignatureEncoding::HexUpper, &raw),
            "F2AFD049FABA22AAD592D0D8DCB14543"
        );
        assert_eq!(
            encode(SignatureEncoding::Base64, &raw),
            "8q/QSfq6IqrVktDY3LFFQw=="
        );
    }

    #[test]
    fn render_replaces_all_placeholders_once() {
        let out = render_payload("{{$key}}|{{$secret}}|{{$timestamp}}", "k", "s", 1234);
        assert_eq!(out, "k|s|1234");
    }

    #[test]
    fn render_keeps_unknown_placeholders() {
        let out = render_payload("{{$body}}|{{$key}}", "k", "s", 1);
        assert_eq!(out, "{{$body}}|k");
    }

    #[test]
    fn render_preserves_literal_parts_and_duplicates() {
        // 同一占位符多次出现也要全部替换（对齐 str::replace 语义）；
        // 未知占位符（如 {{$body}}）原样保留。
        let out = render_payload("{{$key}}{{$timestamp}}{{...}}{{$timestamp}}", "k", "s", 7);
        assert_eq!(out, "k7{{...}}7");
    }

    #[test]
    fn apply_signature_injects_three_headers() {
        let mut headers = vec![];
        apply_signature(&mut headers, &cfg()).unwrap();
        let names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, vec!["App-Key", "App-Timestamp", "App-Sig"]);
        let ts: i64 = headers[1].1.parse().unwrap();
        // 时间戳接近当前时刻（毫秒）。
        let diff = (chrono::Utc::now().timestamp_millis() - ts).abs();
        assert!(diff < 5_000, "时间戳应为发送前最近时刻，偏差 {diff}ms");
        assert_eq!(headers[2].1.len(), 32);
    }

    #[test]
    fn hmac_sha256_matches_openssl_vector() {
        // 对照 `printf 'app-123sec-4561' | openssl dgst -sha256 -hmac 'sec-456'`。
        let c = DynamicSignatureConfig {
            app_key: "app-123".into(),
            app_secret: "sec-456".into(),
            algorithm: SignatureAlgorithm::HmacSHA256,
            ..Default::default()
        };
        let payload = render_payload(&c.payload_template, "app-123", "sec-456", 1);
        let raw = digest(SignatureAlgorithm::HmacSHA256, "sec-456", &payload);
        let sig = encode(SignatureEncoding::HexLower, &raw);
        assert_eq!(sig.len(), 64);
        assert_eq!(
            sig,
            "6aaa2501833b57ee51cbf26df73da91c6dbf923bdef28c678302a4b6b581414c"
        );
    }

    #[test]
    fn empty_key_secret_are_errors() {
        let mut c = cfg();
        c.app_key.clear();
        assert!(matches!(
            apply_signature(&mut vec![], &c),
            Err(AppError::Validation(_))
        ));
        let mut c = cfg();
        c.app_secret.clear();
        assert!(matches!(
            apply_signature(&mut vec![], &c),
            Err(AppError::Validation(_))
        ));
    }
}
