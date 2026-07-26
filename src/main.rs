use bitcask::BitCask;

fn main() {
    let data_dir: &'static str = "data";
    let mut db = BitCask::new(data_dir).unwrap();

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
