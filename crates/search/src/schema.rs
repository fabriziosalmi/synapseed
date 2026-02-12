//! Tantivy index schema for semantic code search.
//!
//! Fields:
//!   - file_path:    Stored STRING — exact file location
//!   - symbol_name:  TEXT (indexed + stored) — for name-based matching
//!   - kind:         STRING (stored + indexed) — faceted: Function, Struct, Enum, ...
//!   - signature:    TEXT (stored) — first line / function signature
//!   - doc_comment:  TEXT (indexed) — crucial for semantic search
//!   - body_snippet: TEXT (stored) — first 5 lines of the symbol body
//!   - line_start:   u64 (stored + fast) — for jump-to-source
//!   - line_end:     u64 (stored + fast)

use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED, STRING,
};

/// All field handles for the search index.
#[derive(Clone)]
pub struct SearchFields {
    pub file_path: Field,
    pub symbol_name: Field,
    pub kind: Field,
    pub signature: Field,
    pub doc_comment: Field,
    pub body_snippet: Field,
    pub line_start: Field,
    pub line_end: Field,
    pub last_modified_epoch: Field,
}

/// Build the Tantivy schema and return (schema, field handles).
pub fn build_schema() -> (Schema, SearchFields) {
    let mut builder = Schema::builder();

    let file_path = builder.add_text_field("file_path", STRING | STORED);

    // symbol_name: heavily indexed for name matching
    let name_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("en_stem")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let symbol_name = builder.add_text_field("symbol_name", name_opts);

    let kind = builder.add_text_field("kind", STRING | STORED);

    let sig_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("en_stem")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let signature = builder.add_text_field("signature", sig_opts);

    // doc_comment: indexed for semantic search, not stored (saves space)
    let doc_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("en_stem")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let doc_comment = builder.add_text_field("doc_comment", doc_opts);

    let body_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("en_stem")
            .set_index_option(IndexRecordOption::WithFreqs),
    );
    let body_snippet = builder.add_text_field("body_snippet", body_opts);

    let line_start = builder.add_u64_field("line_start", FAST | STORED);
    let line_end = builder.add_u64_field("line_end", FAST | STORED);
    let last_modified_epoch = builder.add_u64_field("last_modified_epoch", FAST | STORED);

    let schema = builder.build();

    let fields = SearchFields {
        file_path,
        symbol_name,
        kind,
        signature,
        doc_comment,
        body_snippet,
        line_start,
        line_end,
        last_modified_epoch,
    };

    (schema, fields)
}

/// Recover field handles from an existing schema (e.g., opened from disk).
pub fn fields_from_schema(schema: &Schema) -> Option<SearchFields> {
    Some(SearchFields {
        file_path: schema.get_field("file_path").ok()?,
        symbol_name: schema.get_field("symbol_name").ok()?,
        kind: schema.get_field("kind").ok()?,
        signature: schema.get_field("signature").ok()?,
        doc_comment: schema.get_field("doc_comment").ok()?,
        body_snippet: schema.get_field("body_snippet").ok()?,
        line_start: schema.get_field("line_start").ok()?,
        line_end: schema.get_field("line_end").ok()?,
        last_modified_epoch: schema.get_field("last_modified_epoch").ok()?,
    })
}
