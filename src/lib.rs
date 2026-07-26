use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BITCASK_DATA_FILE_NAME: &'static str = "bitcask.data";

type KeyType = Vec<u8>;
type ValueType = Vec<u8>;

#[derive(Debug, PartialEq)]
pub enum BitCaskError {
    KeyEmptyError,
    KeyNotFoundError,
    ValueEmptyError,
    IO(String),
}

impl From<std::io::Error> for BitCaskError {
    fn from(err: std::io::Error) -> Self {
        BitCaskError::IO(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BitCaskError>;

#[derive(Debug)]
struct ValueLocation {
    value_size: usize,
    value_pos: u64,
}

type KeyDir = BTreeMap<KeyType, ValueLocation>;

// TODO: write unit test.
fn serialize_datafile_entry(key: &[u8], value: &[u8]) -> Vec<u8> {
    let length = 2 * size_of::<u32>() + key.len() + value.len();
    let mut buf = Vec::with_capacity(length);
    buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
    buf.extend_from_slice(key);
    buf.extend_from_slice(value);
    buf
}

struct DataFile {
    path: PathBuf,
    file: File,
}

impl DataFile {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Result<Self> {
        let path = data_dir.as_ref().join(BITCASK_DATA_FILE_NAME);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self { path, file })
    }
    fn write_entry(&mut self, key: &KeyType, val: &ValueType) -> Result<ValueLocation> {
        let buf = serialize_datafile_entry(key, val);
        self.file.write_all(&buf)?;
        Ok(ValueLocation {
            value_size: val.len(),
            value_pos: self.file.metadata()?.len() - val.len() as u64,
        })
    }
}

pub struct BitCask {
    data_file: DataFile,
    key_dir: KeyDir,
}

impl BitCask {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Result<Self> {
        std::fs::create_dir_all(data_dir.as_ref())?;
        let mut data_file = DataFile::new(data_dir.as_ref())?;
        let mut r = BufReader::new(&mut data_file.file);
        let key_dir = Self::build_keydir(&mut r)?;
        let bitcask = Self { data_file, key_dir };
        Ok(bitcask)
    }
    pub fn get(&mut self, key: &KeyType) -> Result<ValueType> {
        if key.is_empty() {
            return Err(BitCaskError::KeyEmptyError);
        }
        match self.key_dir.get(key) {
            Some(location) => {
                let mut value = vec![0; location.value_size];
                self.data_file
                    .file
                    .seek(SeekFrom::Start(location.value_pos))?;
                self.data_file.file.read_exact(&mut value)?;
                Ok(value)
            }
            None => Err(BitCaskError::KeyNotFoundError),
        }
    }
    pub fn put(&mut self, key: KeyType, val: ValueType) -> Result<()> {
        if key.is_empty() {
            return Err(BitCaskError::KeyEmptyError);
        }
        if val.is_empty() {
            return Err(BitCaskError::ValueEmptyError);
        }
        let value_location = self.data_file.write_entry(&key, &val)?;
        self.key_dir.insert(key, value_location);
        Ok(())
    }
    pub fn delete(&mut self, key: &KeyType) -> Result<()> {
        if key.is_empty() {
            return Err(BitCaskError::KeyEmptyError);
        }
        if !self.key_dir.contains_key(key) {
            return Err(BitCaskError::KeyNotFoundError);
        }
        // use an empty value as tombstone.
        let val = Vec::new();
        self.data_file.write_entry(key, &val)?;
        self.key_dir.remove(key);
        Ok(())
    }
    pub fn build_keydir<T: Seek + Read>(r: &mut BufReader<T>) -> Result<KeyDir> {
        let mut len_buf: [u8; 4] = [0; 4];
        let mut keydir = KeyDir::new();
        let file_len = r.seek(SeekFrom::End(0)).unwrap();
        let mut offset = r.seek(SeekFrom::Start(0))?;

        while offset < file_len {
            r.read_exact(&mut len_buf)?;
            let key_len = u32::from_le_bytes(len_buf);
            r.read_exact(&mut len_buf)?;
            let vpos = match u32::from_le_bytes(len_buf) {
                0 => None,
                pos => Some(ValueLocation {
                    value_size: pos as usize,
                    value_pos: offset + 2 * size_of::<u32>() as u64 + key_len as u64,
                }),
            };
            let mut key = vec![0; key_len as usize];
            r.read_exact(&mut key)?;
            match vpos {
                None => {
                    keydir.remove(&key).unwrap();
                }
                Some(vpos) => {
                    r.seek_relative(vpos.value_size as i64)?;
                    offset += vpos.value_size as u64;
                    keydir.insert(key, vpos);
                }
            }
            offset += 2 * size_of::<u32>() as u64 + key_len as u64;
        }
        // No incomplete entry
        assert_eq!(offset, file_len);

        Ok(keydir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_from_existing_file() -> Result<()> {
        let tmp_dir = tempdir()?;

        let k = KeyType::from("foo".as_bytes());
        let v = ValueType::from("bar".as_bytes());
        {
            // Preapare a datafile
            let mut tmp_db = BitCask::new(tmp_dir.path())?;
            tmp_db.put(k.clone(), v.clone())?;
            // Drop DB here
        }

        let mut test_db = BitCask::new(tmp_dir.path())?;
        let result = test_db.get(&k).unwrap();
        assert_eq!(v, result);
        Ok(())
    }

    #[test]
    fn simple_put_get() -> Result<()> {
        let tmp_dir = tempdir()?;
        let mut test_db = BitCask::new(tmp_dir)?;

        let k = KeyType::from("foo".as_bytes());
        let v = ValueType::from("bar".as_bytes());
        test_db.put(k.clone(), v.clone())?;

        let result = test_db.get(&k)?;
        assert_eq!(v, result);
        Ok(())
    }

    #[test]
    fn get_non_exist_key() -> Result<()> {
        let tmp_dir = tempdir()?;
        let mut test_db = BitCask::new(tmp_dir)?;

        let k = KeyType::from("foo".as_bytes());
        let err = test_db.get(&k).expect_err("Key should not exist.");
        assert_eq!(err, BitCaskError::KeyNotFoundError);
        Ok(())
    }

    #[test]
    fn delete_key() -> Result<()> {
        let tmp_dir = tempdir()?;
        let mut test_db = BitCask::new(tmp_dir)?;

        let k = KeyType::from("foo".as_bytes());
        let v = ValueType::from("bar".as_bytes());
        test_db.put(k.clone(), v.clone())?;
        test_db.delete(&k)?;

        let err = test_db.get(&k).expect_err("Key should not exist.");
        assert_eq!(err, BitCaskError::KeyNotFoundError);
        Ok(())
    }

    #[test]
    fn reject_get_empty_key() -> Result<()> {
        let tmp_dir = tempdir()?;
        let mut test_db = BitCask::new(tmp_dir)?;

        let k = KeyType::from("".as_bytes());
        let err = test_db.get(&k).expect_err("Empty key should be rejected.");
        assert_eq!(err, BitCaskError::KeyEmptyError);
        Ok(())
    }

    #[test]
    fn reject_put_empty_key() -> Result<()> {
        let tmp_dir = tempdir()?;
        let mut test_db = BitCask::new(tmp_dir)?;

        let k = KeyType::from("".as_bytes());
        let v = ValueType::from("bar".as_bytes());
        let err = test_db
            .put(k.clone(), v.clone())
            .expect_err("Empty key should be rejected.");
        assert_eq!(err, BitCaskError::KeyEmptyError);
        Ok(())
    }

    #[test]
    fn reject_delete_empty_key() -> Result<()> {
        let tmp_dir = tempdir()?;
        let mut test_db = BitCask::new(tmp_dir)?;

        let k = KeyType::from("".as_bytes());
        let err = test_db
            .delete(&k)
            .expect_err("Empty key should be rejected.");
        assert_eq!(err, BitCaskError::KeyEmptyError);
        Ok(())
    }
}
