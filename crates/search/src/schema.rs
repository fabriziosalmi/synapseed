//! Tantivy index schema for semantic code search (v4.11.0+).
//!
//! Fields:
//!   - file_path:    Stored STRING — exact file location
//!   - symbol_name:  TEXT (indexed + stored, tokenizer: "code") — name-based matching
//!   - kind:         STRING (stored + indexed) — faceted: Function, Struct, Enum, ...
//!   - signature:    TEXT (stored, tokenizer: "code") — function signature
//!   - doc_comment:  TEXT (indexed, tokenizer: "en_stem") — semantic search (prose)
//!   - body_snippet: TEXT (stored, tokenizer: "en_stem") — first 30 lines of body
//!   - line_start:   u64 (stored + fast) — for jump-to-source
//!   - line_end:     u64 (stored + fast)
//!   - visibility:   STRING (stored + indexed) — public/crate/super/private/unknown
//!   - _schema_v:    u64 (stored) — schema version sentinel for disk index migration

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
    pub visibility: Field,
    pub schema_version: Field,
}

/// Current schema version. Bump when tokenizer or field layout changes.
pub const SCHEMA_VERSION: u64 = 2;

/// Build the Tantivy schema and return (schema, field handles).
pub fn build_schema() -> (Schema, SearchFields) {
    let mut builder = Schema::builder();

    let file_path = builder.add_text_field("file_path", STRING | STORED);

    // symbol_name: "code" tokenizer (CamelCase + snake_case splitting, no stemming)
    let name_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("code")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let symbol_name = builder.add_text_field("symbol_name", name_opts);

    let kind = builder.add_text_field("kind", STRING | STORED);

    // signature: "code" tokenizer (same as symbol_name — code identifiers)
    let sig_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("code")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let signature = builder.add_text_field("signature", sig_opts);

    // doc_comment: "en_stem" for natural-language semantic search (prose)
    let doc_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("en_stem")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let doc_comment = builder.add_text_field("doc_comment", doc_opts);

    // body_snippet: "en_stem" — body text is a mix of code and prose
    let body_opts = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("en_stem")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let body_snippet = builder.add_text_field("body_snippet", body_opts);

    let line_start = builder.add_u64_field("line_start", FAST | STORED);
    let line_end = builder.add_u64_field("line_end", FAST | STORED);
    let last_modified_epoch = builder.add_u64_field("last_modified_epoch", FAST | STORED);

    // Visibility (v4.9.0): stored + indexed as STRING for faceted filtering
    let visibility = builder.add_text_field("visibility", STRING | STORED);

    // Schema version sentinel (v4.11.0): forces disk index recreation on upgrade
    let schema_version = builder.add_u64_field("_schema_v", STORED);

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
        visibility,
        schema_version,
    };

    (schema, fields)
}

/// Recover field handles from an existing schema (e.g., opened from disk).
/// Returns `None` if any field is missing (triggers index recreation).
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
        visibility: schema.get_field("visibility").ok()?,
        schema_version: schema.get_field("_schema_v").ok()?,
    })
}
