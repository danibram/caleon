use anyhow::{bail, ensure, Context, Result};
use chacha20poly1305::{
    aead::{rand_core::RngCore, Aead, OsRng, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn initialize(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS credentials (
           name TEXT PRIMARY KEY, version INTEGER NOT NULL,
           nonce BLOB NOT NULL, ciphertext BLOB NOT NULL);",
    )?;
    Ok(())
}

pub(crate) struct Credentials<'a> {
    db: &'a Connection,
    key_path: PathBuf,
    external: bool,
}

impl<'a> Credentials<'a> {
    pub fn new(db: &'a Connection, data_dir: &Path) -> Self {
        let external = std::env::var_os("CALEON_SECRETS_KEY_FILE");
        Self {
            db,
            key_path: external
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| data_dir.join("secrets.key")),
            external: external.is_some(),
        }
    }

    fn key(&self, create: bool) -> Result<[u8; 32]> {
        match fs::read(&self.key_path) {
            Ok(bytes) => {
                return bytes
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("master key must contain exactly 32 bytes"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => bail!("cannot read credential master key"),
        }
        ensure!(
            create && !self.external,
            "credential master key is missing; restore the original key file"
        );
        let count: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM credentials", [], |r| r.get(0))?;
        ensure!(
            count == 0,
            "credential master key is missing; restore the original key file"
        );
        let mut key = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut key)
            .map_err(|_| anyhow::anyhow!("cannot generate master key"))?;
        let mut suffix = [0u8; 16];
        OsRng
            .try_fill_bytes(&mut suffix)
            .map_err(|_| anyhow::anyhow!("cannot generate key file name"))?;
        let tmp = self
            .key_path
            .with_extension(format!("{:032x}.tmp", u128::from_ne_bytes(suffix)));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| -> Result<()> {
            let mut file = options.open(&tmp).context("create master key file")?;
            file.write_all(&key)?;
            file.sync_all()?;
            drop(file);
            match fs::hard_link(&tmp, &self.key_path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                // some NAS shares (SMB, exFAT) have no hard links: write the key in place, still
                // refusing to replace an existing file
                Err(_) => {
                    let mut file = options
                        .open(&self.key_path)
                        .context("cannot install master key file")?;
                    if let Err(e) = file.write_all(&key).and_then(|()| file.sync_all()) {
                        drop(file);
                        let _ = fs::remove_file(&self.key_path);
                        return Err(e).context("cannot install master key file");
                    }
                }
            }
            #[cfg(unix)]
            fs::File::open(self.key_path.parent().unwrap_or(Path::new(".")))?.sync_all()?;
            Ok(())
        })();
        let _ = fs::remove_file(tmp);
        result?;
        self.key(false)
    }

    pub fn get(&self, name: &str) -> Result<Option<String>> {
        let row = self
            .db
            .query_row(
                "SELECT version, nonce, ciphertext FROM credentials WHERE name = ?1",
                [name],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Vec<u8>>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((version, nonce, ciphertext)) = row else {
            return Ok(None);
        };
        ensure!(
            version == 1 && nonce.len() == 24,
            "unsupported or damaged encrypted credential"
        );
        let cipher = XChaCha20Poly1305::new((&self.key(false)?).into());
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| {
                anyhow::anyhow!("cannot decrypt credential; check the master key and database")
            })?;
        Ok(Some(
            String::from_utf8(plaintext).context("invalid decrypted credential")?,
        ))
    }

    pub fn set(&self, name: &str, secret: &str) -> Result<()> {
        // Verify existing ciphertext before allowing a replacement with another master key.
        self.get(name)?;
        let existing: Option<String> = self
            .db
            .query_row("SELECT name FROM credentials LIMIT 1", [], |r| r.get(0))
            .optional()?;
        if let Some(existing) = existing {
            self.get(&existing)?;
        }
        let cipher = XChaCha20Poly1305::new((&self.key(true)?).into());
        let mut nonce = [0u8; 24];
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| anyhow::anyhow!("cannot generate credential nonce"))?;
        let ciphertext = cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: secret.as_bytes(),
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("cannot encrypt credential"))?;
        self.db.execute(
            "INSERT INTO credentials (name, version, nonce, ciphertext) VALUES (?1, 1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET version=1, nonce=excluded.nonce, ciphertext=excluded.ciphertext",
            params![name, &nonce[..], ciphertext],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "caleon-credentials-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn generic_credentials_use_fresh_nonces_and_bind_ciphertext_to_its_name() {
        let dir = TestDir::new();
        let db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let store = Credentials {
            db: &db,
            key_path: dir.0.join("key"),
            external: false,
        };
        store.set("weather.aemet.api_key", "secret-a").unwrap();
        store.set("future.provider.token", "secret-b").unwrap();
        assert_eq!(
            store.get("future.provider.token").unwrap().as_deref(),
            Some("secret-b")
        );
        assert_eq!(store.get("absent").unwrap(), None);
        let first: Vec<u8> = db
            .query_row(
                "SELECT nonce FROM credentials WHERE name=?1",
                ["weather.aemet.api_key"],
                |r| r.get(0),
            )
            .unwrap();
        store.set("weather.aemet.api_key", "secret-a").unwrap();
        let next: Vec<u8> = db
            .query_row(
                "SELECT nonce FROM credentials WHERE name=?1",
                ["weather.aemet.api_key"],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(first, next);
        db.execute(
            "UPDATE credentials SET name='swapped' WHERE name='future.provider.token'",
            [],
        )
        .unwrap();
        assert!(store.get("swapped").is_err());
    }

    #[test]
    fn external_master_key_is_required_and_never_generated_or_replaced() {
        let dir = TestDir::new();
        let db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let path = dir.0.join("mounted-secret");
        let store = Credentials {
            db: &db,
            key_path: path.clone(),
            external: true,
        };
        assert!(store.set("provider.token", "secret").is_err());
        assert!(!path.exists());
        let key = [7u8; 32];
        fs::write(&path, key).unwrap();
        store.set("provider.token", "secret").unwrap();
        assert_eq!(
            store.get("provider.token").unwrap().as_deref(),
            Some("secret")
        );
        assert_eq!(fs::read(path).unwrap(), key);
        assert!(!dir.0.join("secrets.key").exists());
        fs::write(&store.key_path, [9u8; 32]).unwrap();
        assert!(store.set("another.provider.token", "secret").is_err());
        assert!(store.get("another.provider.token").unwrap().is_none());
    }
}
