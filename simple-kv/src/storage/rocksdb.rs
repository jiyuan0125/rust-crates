use std::convert::TryInto;

use crate::{KvError, Kvpair, Storage};
use rocksdb::{Options, DB};

pub struct RocksDb(DB);

impl RocksDb {
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> Self {
        Self(DB::open_default(path).unwrap())
    }

    pub fn get_or_create_table(&self, name: &str) -> &rocksdb::ColumnFamily {
        if self.0.cf_handle(name).is_none() {
            // self.0.create_cf(name, &Options::default()).unwrap();
            todo!()
        }
        self.0.cf_handle(name).unwrap()
    }
}

impl Storage for RocksDb {
    fn get(&self, table: &str, key: &str) -> Result<Option<crate::Value>, KvError> {
        let cf = self.get_or_create_table(table);
        self.0
            .get_cf(&cf, key)
            .map_err(|e| KvError::Internal(e.to_string()))?
            .map(|v| (&v[..]).try_into())
            .transpose()
    }

    fn set(
        &self,
        table: &str,
        key: String,
        value: crate::Value,
    ) -> Result<Option<crate::Value>, KvError> {
        let cf = self.get_or_create_table(table);
        let old: Option<Result<crate::Value, _>> = self
            .0
            .get_cf(&cf, key.as_bytes())
            .map_err(|e| KvError::Internal(e.to_string()))?
            .map(|v| (&v[..]).try_into());
        let val: Vec<u8> = value.try_into()?;
        let val: &[u8] = val.as_ref();
        self.0.put_cf(&cf, key.as_bytes(), val).unwrap();
        old.transpose()
    }

    fn contains(&self, table: &str, key: &str) -> Result<bool, KvError> {
        let cf = self.get_or_create_table(table);
        Ok(self.0.key_may_exist_cf(&cf, key))
    }

    fn del(&self, table: &str, key: &str) -> Result<Option<crate::Value>, KvError> {
        let cf = self.get_or_create_table(table);
        let old: Option<Result<crate::Value, _>> = self
            .0
            .get_cf(&cf, key.as_bytes())
            .map_err(|e| KvError::Internal(e.to_string()))?
            .map(|v| (&v[..]).try_into());
        self.0
            .delete_cf(&cf, key)
            .map_err(|e| KvError::Internal(e.to_string()))?;
        old.transpose()
    }

    fn get_all(&self, table: &str) -> Result<Vec<crate::Kvpair>, KvError> {
        let cf = self.get_or_create_table(table);
        Ok(self
            .0
            .iterator_cf(&cf, rocksdb::IteratorMode::Start)
            .map(|v| v.map(|it| it.into()))
            .flatten()
            .collect())
    }

    fn get_iter(&self, table: &str) -> Result<Box<dyn Iterator<Item = crate::Kvpair>>, KvError> {
        let cf = self.get_or_create_table(table);
        let iter = self.0.iterator_cf(&cf, rocksdb::IteratorMode::Start);
        // Ok(Box::new(iter.map(|v| v.map(|it| it.into())).flatten()))
        todo!()
    }

    // fn get_iter<'a>(
    //     &'a self,
    //     table: &str,
    // ) -> Result<Box<dyn Iterator<Item = crate::Kvpair> + 'a>, crate::KvError> {
    //     let cf = self.get_or_create_table(table);
    //     let iter = self.0.iterator_cf(&cf, rocksdb::IteratorMode::Start);
    //     let iter = Iter::new(iter);
    //     Ok(Box::new(iter))
    // }
}

impl From<(std::boxed::Box<[u8]>, std::boxed::Box<[u8]>)> for Kvpair {
    fn from(value: (std::boxed::Box<[u8]>, std::boxed::Box<[u8]>)) -> Self {
        Kvpair::new(
            std::str::from_utf8(value.0.as_ref()).unwrap(),
            value.1.as_ref().try_into().unwrap(),
        )
    }
}
