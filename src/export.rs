use std::{error::Error, path::Path};

use rust_xlsxwriter::{Format, Workbook, XlsxError};

use crate::parser::TableData;

type ExportResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub fn to_tsv(data: &TableData) -> String {
    let mut lines = Vec::with_capacity(data.rows.len() + 1);

    let headers: Vec<&str> = data.columns.iter().map(|c| c.name.as_str()).collect();
    lines.push(headers.join("\t"));

    for row in &data.rows {
        let cells: Vec<&str> = table_row_cells(data, row).collect();
        lines.push(cells.join("\t"));
    }

    let mut tsv = lines.join("\n");
    tsv.push('\n');
    tsv
}

pub fn to_csv(data: &TableData) -> ExportResult<String> {
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());

    writer.write_record(data.columns.iter().map(|c| c.name.as_str()))?;
    for row in &data.rows {
        writer.write_record(table_row_cells(data, row))?;
    }

    let bytes = writer.into_inner()?;
    Ok(String::from_utf8(bytes)?)
}

pub fn to_html_table(data: &TableData) -> String {
    let mut html = String::from("<table>\n<thead>\n<tr>");

    for column in &data.columns {
        html.push_str("<th>");
        html.push_str(&escape_html(&column.name));
        html.push_str("</th>");
    }

    html.push_str("</tr>\n</thead>\n<tbody>\n");

    for row in &data.rows {
        html.push_str("<tr>");
        for cell in table_row_cells(data, row) {
            html.push_str("<td>");
            html.push_str(&escape_html(cell));
            html.push_str("</td>");
        }
        html.push_str("</tr>\n");
    }

    html.push_str("</tbody>\n</table>\n");
    html
}

pub fn save_xlsx(data: &TableData, path: impl AsRef<Path>) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("CrabPaste")?;

    let header_format = Format::new().set_bold();

    for (col_idx, column) in data.columns.iter().enumerate() {
        worksheet.write_string_with_format(0, col_idx as u16, &column.name, &header_format)?;
    }

    for (row_idx, row) in data.rows.iter().enumerate() {
        for (col_idx, cell) in table_row_cells(data, row).enumerate() {
            worksheet.write_string((row_idx + 1) as u32, col_idx as u16, cell)?;
        }
    }

    worksheet.set_freeze_panes(1, 0)?;
    for col_idx in 0..data.columns.len() {
        worksheet.set_column_width(col_idx as u16, estimate_column_width(data, col_idx))?;
    }

    workbook.save(path)
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn table_row_cells<'a>(
    data: &'a TableData,
    row: &'a [String],
) -> impl Iterator<Item = &'a str> + 'a {
    (0..data.columns.len()).map(|idx| row.get(idx).map_or("", String::as_str))
}

fn estimate_column_width(data: &TableData, col_idx: usize) -> f64 {
    let header_width = display_width(&data.columns[col_idx].name);
    let sample_width = data
        .rows
        .iter()
        .take(200)
        .filter_map(|row| row.get(col_idx))
        .map(|cell| display_width(cell))
        .max()
        .unwrap_or(0);

    ((header_width.max(sample_width) as f64) + 2.0).clamp(8.0, 48.0)
}

fn display_width(value: &str) -> usize {
    value
        .chars()
        .map(|ch| if ch.is_ascii() { 1 } else { 2 })
        .sum()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::parser::{ColumnInfo, DataFormatLabel};

    fn sample_table() -> TableData {
        TableData {
            source_format: DataFormatLabel::Bi,
            columns: vec![
                ColumnInfo {
                    id: "name".into(),
                    name: "名称".into(),
                    location: String::new(),
                    col_type: String::new(),
                },
                ColumnInfo {
                    id: "note".into(),
                    name: "备注".into(),
                    location: String::new(),
                    col_type: String::new(),
                },
                ColumnInfo {
                    id: "empty".into(),
                    name: "空".into(),
                    location: String::new(),
                    col_type: String::new(),
                },
            ],
            rows: vec![vec![
                "中文".into(),
                "a,b \"q\"\n下一行 <tag & 'x'>".into(),
                String::new(),
            ]],
        }
    }

    #[test]
    fn test_tsv_export() {
        let tsv = to_tsv(&sample_table());
        assert_eq!(
            tsv,
            "名称\t备注\t空\n中文\ta,b \"q\"\n下一行 <tag & 'x'>\t\n"
        );
    }

    #[test]
    fn test_csv_export_escapes_special_cells() {
        let csv = to_csv(&sample_table()).unwrap();
        assert!(csv.starts_with("名称,备注,空\n"));
        assert!(csv.contains("\"a,b \"\"q\"\"\n下一行 <tag & 'x'>\""));
        assert!(csv.ends_with('\n'));
    }

    #[test]
    fn test_html_export_escapes_cells() {
        let html = to_html_table(&sample_table());
        assert!(html.contains("<table>"));
        assert!(html.contains("a,b &quot;q&quot;\n下一行 &lt;tag &amp; &#39;x&#39;&gt;"));
    }

    #[test]
    fn test_xlsx_export_writes_zip_file() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "crabpaste-export-test-{}-{nonce}.xlsx",
            std::process::id()
        ));

        save_xlsx(&sample_table(), &path).unwrap();
        let bytes = fs::read(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert!(bytes.starts_with(b"PK"));
        assert!(bytes.len() > 100);
    }
}
