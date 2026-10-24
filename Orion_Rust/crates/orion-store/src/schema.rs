//! Esquema SQLite da memória: o mesmo do Python (`orion/memory/schema.py`), versão a versão.
//!
//! Os arquivos `sql/vNN.sql` foram gerados do Python e não devem ser editados à mão: o banco é
//! compartilhado durante a transição.

pub const SCHEMA_VERSION: u32 = 24;
pub const TOKENIZER: &str = "unicode61 remove_diacritics 2";

const V: [&str; 24] = [
    include_str!("../sql/v01.sql"),
    include_str!("../sql/v02.sql"),
    include_str!("../sql/v03.sql"),
    include_str!("../sql/v04.sql"),
    include_str!("../sql/v05.sql"),
    include_str!("../sql/v06.sql"),
    include_str!("../sql/v07.sql"),
    include_str!("../sql/v08.sql"),
    include_str!("../sql/v09.sql"),
    include_str!("../sql/v10.sql"),
    include_str!("../sql/v11.sql"),
    include_str!("../sql/v12.sql"),
    include_str!("../sql/v13.sql"),
    include_str!("../sql/v14.sql"),
    include_str!("../sql/v15.sql"),
    include_str!("../sql/v16.sql"),
    include_str!("../sql/v17.sql"),
    include_str!("../sql/v18.sql"),
    include_str!("../sql/v19.sql"),
    include_str!("../sql/v20.sql"),
    include_str!("../sql/v21.sql"),
    include_str!("../sql/v22.sql"),
    include_str!("../sql/v23.sql"),
    include_str!("../sql/v24.sql"),
];

/// DDL completo de um banco novo.
pub fn ddl() -> String {
    V.iter().map(|s| format!("{s}\n")).collect()
}

/// SQL que sobe o banco da versão `de` para `de + 1`.
pub fn migration(de: u32) -> Option<&'static str> {
    if de == 0 {
        return None;
    }
    V.get(de as usize).copied()
}
