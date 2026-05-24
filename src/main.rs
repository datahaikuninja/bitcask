use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BITCASK_DATA_DIR: &'static str = "data";
const BITCASK_DATA_FILE_NAME: &'static str = "bitcask.data";

type KeyType = Vec<u8>;
type ValueType = Vec<u8>;

#[derive(Debug)]
pub enum BitCaskError {
    BitCaskError,
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
    pub fn new(data_file_path: &str) -> Result<Self> {
        std::fs::create_dir_all(BITCASK_DATA_DIR)?;
        let path = Path::new(BITCASK_DATA_DIR).join(data_file_path);
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

struct BitCask {
    data_file: DataFile,
    key_dir: KeyDir,
}

impl BitCask {
    pub fn new() -> Result<Self> {
        println!("start BitCask");
        let data_file = DataFile::new(BITCASK_DATA_FILE_NAME)?;
        let key_dir = KeyDir::new();

        // TODO: build the KeyDir when the BitCask start.

        Ok(Self { data_file, key_dir })
    }
    pub fn get(&mut self, key: &KeyType) -> Result<ValueType> {
        match self.key_dir.get(key) {
            Some(location) => {
                let mut value = vec![0; location.value_size];
                self.data_file
                    .file
                    .seek(SeekFrom::Start(location.value_pos))?;
                self.data_file.file.read_exact(&mut value)?;
                Ok(value)
            }
            None => Err(BitCaskError::BitCaskError),
        }
    }
    pub fn put(&mut self, key: KeyType, val: ValueType) -> Result<()> {
        if val.is_empty() {
            return Err(BitCaskError::BitCaskError);
        }
        let value_location = self.data_file.write_entry(&key, &val)?;
        self.key_dir.insert(key, value_location);
        Ok(())
    }
    pub fn delete(&mut self, key: &KeyType) -> Result<()> {
        if !self.key_dir.contains_key(key) {
            return Err(BitCaskError::BitCaskError);
        }
        // use an empty value as tombstone.
        let val = Vec::new();
        self.data_file.write_entry(key, &val)?;
        self.key_dir.remove(key);
        Ok(())
    }
}

fn main() {
    let mut db = BitCask::new().unwrap();

    let k = Vec::from("foo".as_bytes());
    let v = Vec::from("bar".as_bytes());
    db.put(k.clone(), v.clone()).unwrap();

    let result = db.get(&k).unwrap();
    let key_as_str = String::from_utf8(k.clone()).unwrap();
    let result_as_str = String::from_utf8(result.clone()).unwrap();
    println!("Read {}: {}", &key_as_str, &result_as_str);
    assert_eq!(v, result);

    db.delete(&k).unwrap();
    db.get(&k).expect_err("The key should not exist.");

    println!("Done!");
}
