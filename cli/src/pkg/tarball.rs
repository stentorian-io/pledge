use anyhow::{bail, Result};
use flate2::bufread::GzDecoder;
use hmac_sha512::Hash;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use tar::Archive;
use url::Url;

pub struct PkgTarball {
    name: String,
    dir: PathBuf,
    source_url: Url,
    checksum: Vec<u8>,
}

impl PkgTarball {
    pub fn new(name: String, dir: PathBuf, source_url: Url, checksum: Vec<u8>) -> Result<Self> {
        Ok(PkgTarball {
            name,
            dir,
            source_url,
            checksum,
        })
    }

    pub fn download_to_disk_if_needed(&self) -> Result<PathBuf> {
        let tarball_path = self.path();

        if tarball_path.is_file() {
            let tarball_data = fs::read(&tarball_path)?;

            if self.is_integrity_ok(&tarball_data) {
                println!("Valid tarball exists on file system. Will use existing...");

                return Ok(tarball_path);
            }

            println!("Found existing tarball but integrity check failed. Will remove existing...");
            fs::remove_file(&tarball_path)?;
        }

        println!("Downloading tarball from registry...");

        let response = reqwest::blocking::get(self.source_url.as_str())?.error_for_status();

        if let Err(error) = response {
            bail!(error);
        }

        let tarball_data = response.unwrap().bytes()?;

        if !self.is_integrity_ok(&tarball_data.to_vec()) {
            bail!("Could not verify integrity of downloaded tarball.");
        }

        println!("Integrity OK, storing on the file system...");

        if !self.dir.is_dir() {
            fs::create_dir(&self.dir)?;
        }

        fs::write(&tarball_path, tarball_data)?;
        Ok(tarball_path)
    }

    pub fn unpack_into(
        &self,
        pkg_config: &mut String,
        pkg_files: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        let tarball_buffer = fs::read(self.path())?;
        let tarball_decoder = GzDecoder::new(tarball_buffer.as_slice());
        let mut tarball_data = Archive::new(tarball_decoder);

        for entry in tarball_data.entries()? {
            let mut entry = entry.unwrap();
            let entry_path = entry.header().path()?.to_path_buf();
            let entry_name = entry_path.file_name().unwrap();

            if entry_name.eq("package.json") {
                entry.read_to_string(pkg_config)?;
            }

            pkg_files.insert(entry_path);
        }

        Ok(())
    }

    fn path(&self) -> PathBuf {
        self.dir.join(&self.name)
    }

    fn is_integrity_ok(&self, buffer: &Vec<u8>) -> bool {
        let mut hash = Hash::new();

        hash.update(buffer);
        hash.finalize().eq(self.checksum.as_slice())
    }
}
