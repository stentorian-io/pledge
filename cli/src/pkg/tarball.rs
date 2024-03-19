use anyhow::{bail, Result};
use flate2::bufread::GzDecoder;
use hmac_sha512::Hash;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use tar::{Archive, Entry};
use url::Url;

pub struct PkgTarball {
    name: String,
    dir: PathBuf,
    source_url: Url,
    checksum: Vec<u8>,
    data: Option<Vec<u8>>,
}

impl PkgTarball {
    pub fn new(name: String, dir: PathBuf, source_url: Url, checksum: Vec<u8>) -> Result<Self> {
        let data = None;

        Ok(Self {
            source_url,
            checksum,
            data,
            name,
            dir,
        })
    }

    pub fn download_if_needed(&mut self) -> Result<()> {
        let tarball_path = self.path();

        if tarball_path.is_file() {
            let tarball_data = fs::read(&tarball_path)?;

            if self.is_integrity_ok(&tarball_data) {
                println!("Valid tarball exists on file system. Will use existing...");

                return self.decode_and_store_data(tarball_data);
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
        let tarball_data_vec = tarball_data.to_vec();

        if !self.is_integrity_ok(&tarball_data_vec) {
            bail!("Could not verify integrity of downloaded tarball.");
        }

        println!("Integrity OK, storing on the file system...");

        if !self.dir.is_dir() {
            fs::create_dir(&self.dir)?;
        }

        fs::write(&tarball_path, &tarball_data)?;
        self.decode_and_store_data(tarball_data_vec)
    }

    pub fn get_files<ShouldInclude>(
        &self,
        should_include: Option<ShouldInclude>,
    ) -> Result<HashSet<PathBuf>>
    where
        ShouldInclude: Fn(&Entry<&[u8]>) -> Result<bool>,
    {
        let mut files = HashSet::new();

        if let Some(data) = &self.data {
            let mut archive = Archive::new(&data[..]);

            for entry in archive.entries()? {
                let entry = entry.unwrap();

                if let Some(should_include) = &should_include {
                    if !should_include(&entry)? {
                        continue;
                    }
                }

                let entry_path = entry
                    .header()
                    .path()?
                    .strip_prefix("package")?
                    .to_path_buf();

                files.insert(entry_path);
            }
        }

        Ok(files)
    }

    pub fn load_file_by_path(&self, file_path: &PathBuf) -> Result<Option<Vec<u8>>> {
        if let Some(data) = &self.data {
            let mut archive = Archive::new(&data[..]);

            for entry in archive.entries()? {
                let mut entry = entry.unwrap();

                let entry_path = entry
                    .header()
                    .path()?
                    .strip_prefix("package")?
                    .to_path_buf();

                let file_path = if file_path.starts_with("./") {
                    file_path.strip_prefix("./")?
                } else if file_path.starts_with("/") {
                    file_path.strip_prefix("/")?
                } else {
                    file_path
                };

                if entry_path.eq(file_path) {
                    let mut buffer = Vec::new();
                    entry.read_to_end(&mut buffer)?;
                    return Ok(Some(buffer));
                }
            }
        }

        Ok(None)
    }

    fn decode_and_store_data(&mut self, data: Vec<u8>) -> Result<()> {
        let mut buffer = Vec::new();
        let mut decoder = GzDecoder::new(&data[..]);

        decoder.read_to_end(&mut buffer)?;

        self.data = Some(buffer);
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
