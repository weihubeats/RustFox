//! 环境变量加密存储（M11）。
//!
//! 使用 AES-256-GCM：随机 32 字节主密钥保存在
//! `{data_dir}/RustFox/master.key`（权限 0600，与数据库同目录），每次加密
//! 生成随机 12 字节 nonce，输出格式 `base64(nonce):base64(ciphertext||tag)`。
//! 兼容性：非加密格式（无 `:` / nonce 非 12 字节）原样返回（旧版本明文
//! 数据容错）；明确加密格式但解密失败（主密钥丢失/更换、密文损坏）返回
//! `SecretError::DecryptionFailed`，由上层提示用户，避免把 base64 密文当
//! 明文继续解析导致环境变量静默丢失。

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use rand::RngCore;
use std::path::PathBuf;
use std::sync::OnceLock;

/// 加密相关错误。
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("密钥文件不可用：{0}")]
    IoError(std::io::Error),
    #[error("非法的密钥文件内容（应为 32 字节 Base64）")]
    InvalidKey,
    #[error("密文格式损坏")]
    InvalidCiphertext,
    #[error("解密失败：主密钥不匹配或密文已损坏")]
    DecryptionFailed,
}

pub type Result<T> = std::result::Result<T, SecretError>;

/// 主密钥（32 字节）。
#[derive(Clone)]
pub struct MasterKey([u8; 32]);

/// 数据目录（与 fox-storage::db::data_dir 一致：Windows 上同为 Roaming，
/// 保证 master.key 与 rustfox.db 同树，用户整目录备份不会漏密钥）。
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RustFox")
}

/// 旧版数据目录（dirs::data_local_dir）：Windows 上与 data_dir 不同树。
/// 仅用于读取旧安装遗留的密钥并迁移到统一目录，macOS/Linux 上两者同路径。
fn legacy_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RustFox")
}

fn key_path() -> PathBuf {
    data_dir().join("master.key")
}

/// 进程内主密钥缓存：环境列表的每行加解密都会取密钥，
/// 缓存避免每次一次同步磁盘 IO。密钥与进程同生命周期、不运行时轮换。
static KEY_CACHE: OnceLock<MasterKey> = OnceLock::new();

/// 确保主密钥存在（不存在则生成并写入，权限 0600），返回主密钥。
///
/// 首次调用后进程内缓存；后续调用零磁盘开销。
pub fn ensure_master_key() -> Result<MasterKey> {
    if let Some(key) = KEY_CACHE.get() {
        return Ok(key.clone());
    }
    let key = load_or_create_key()?;
    let _ = KEY_CACHE.set(key.clone());
    Ok(key)
}

fn load_or_create_key() -> Result<MasterKey> {
    let path = key_path();
    if path.exists() {
        return read_key_file(&path);
    }
    // 兼容旧版安装（Windows 下密钥曾存于 Local 而库在 Roaming）：
    // 读取旧位置并迁移到统一数据目录，迁移失败不影响本次使用
    let legacy = legacy_data_dir().join("master.key");
    if legacy.exists() {
        let key = read_key_file(&legacy)?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::copy(&legacy, &path).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
        return Ok(key);
    }

    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    let encoded = base64::engine::general_purpose::STANDARD.encode(key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SecretError::IoError)?;
    }
    std::fs::write(&path, encoded).map_err(SecretError::IoError)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(MasterKey(key))
}

fn read_key_file(path: &PathBuf) -> Result<MasterKey> {
    let content = std::fs::read_to_string(path).map_err(SecretError::IoError)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content.trim())
        .map_err(|_| SecretError::InvalidKey)?;
    let arr: [u8; 32] = bytes.try_into().map_err(|_| SecretError::InvalidKey)?;
    Ok(MasterKey(arr))
}

fn cipher(key: &MasterKey) -> Aes256Gcm {
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0))
}

/// 加密明文，返回 `base64(nonce):base64(ciphertext||tag)`。
pub fn encrypt(key: &MasterKey, plain: &str) -> Result<String> {
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher(key)
        .encrypt(Nonce::from_slice(&nonce_bytes), plain.as_bytes())
        .map_err(|_| SecretError::InvalidCiphertext)?;
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    Ok(format!("{}:{}", b64(&nonce_bytes), b64(&ciphertext)))
}

/// 解密 `encrypt` 产物。
///
/// - 非加密格式（无 `:`、nonce 非 12 字节的 base64）：视为旧版本明文，原样返回；
/// - 明确加密格式（`base64(12B nonce):base64(ciphertext)`）但解密失败
///   （主密钥不匹配 / 密文损坏）：返回 `SecretError::DecryptionFailed`。
pub fn decrypt(key: &MasterKey, text: &str) -> Result<String> {
    let Some((nonce_b64, cipher_b64)) = text.split_once(':') else {
        return Ok(text.to_string());
    };
    let engine = base64::engine::general_purpose::STANDARD;
    let Ok(nonce) = engine.decode(nonce_b64) else {
        return Ok(text.to_string());
    };
    if nonce.len() != 12 {
        return Ok(text.to_string());
    }
    let Ok(ciphertext) = engine.decode(cipher_b64) else {
        return Err(SecretError::DecryptionFailed);
    };
    let plain = cipher(key)
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| SecretError::DecryptionFailed)?;
    String::from_utf8(plain).map_err(|_| SecretError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(seed: u8) -> MasterKey {
        MasterKey([seed; 32])
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key(7);
        let plain = r#"{"token":"secret-123","users":3}"#;
        let enc = encrypt(&key, plain).unwrap();
        assert_ne!(enc, plain);
        assert!(enc.contains(':'));
        assert_eq!(decrypt(&key, &enc).unwrap(), plain);
    }

    #[test]
    fn ciphertext_differs_each_time() {
        let key = test_key(9);
        let a = encrypt(&key, "same").unwrap();
        let b = encrypt(&key, "same").unwrap();
        assert_ne!(a, b);
        assert_eq!(decrypt(&key, &a).unwrap(), "same");
        assert_eq!(decrypt(&key, &b).unwrap(), "same");
    }

    #[test]
    fn plaintext_passthrough_when_not_encrypted() {
        let key = test_key(1);
        let legacy = r#"{"a":"b"}"#;
        assert_eq!(decrypt(&key, legacy).unwrap(), legacy);
    }

    #[test]
    fn wrong_key_returns_decryption_failed() {
        let key = test_key(2);
        let other = test_key(3);
        let enc = encrypt(&key, "top-secret").unwrap();
        assert!(matches!(
            decrypt(&other, &enc),
            Err(SecretError::DecryptionFailed)
        ));
    }

    #[test]
    fn valid_nonce_bad_ciphertext_errors() {
        // nonce 合法（12 字节 base64）但密文段不是合法 base64 → 判定为损坏密文。
        let key = test_key(5);
        let nonce = base64::engine::general_purpose::STANDARD.encode([0u8; 12]);
        let malformed = format!("{nonce}:!!!not-base64!!!");
        assert!(matches!(
            decrypt(&key, &malformed),
            Err(SecretError::DecryptionFailed)
        ));
    }

    #[test]
    fn malformed_ciphertext_passthrough() {
        let key = test_key(4);
        // nonce 非 12 字节 / 非 base64 → 视为明文原样返回。
        assert_eq!(decrypt(&key, "abc:def").unwrap(), "abc:def");
        assert_eq!(decrypt(&key, "QQ==:YQ==").unwrap(), "QQ==:YQ==");
        // 含冒号的明文 JSON 也被原样保留。
        let json = r#"{"a":"b"}"#;
        assert_eq!(decrypt(&key, json).unwrap(), json);
    }

    #[test]
    fn key_stability_across_instantiations() {
        // 同一密钥内容重复读取（文件已存在时 ensure_master_key 直接复用）。
        let k1 = ensure_master_key().unwrap();
        let enc = encrypt(&k1, "stable").unwrap();
        assert_eq!(decrypt(&k1, &enc).unwrap(), "stable");
    }
}
