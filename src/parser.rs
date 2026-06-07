use serde_json::Value;

pub struct TableData {
    pub source_format: DataFormatLabel,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<String>>,
}

impl TableData {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
}

pub struct ColumnInfo {
    #[allow(unused)]
    pub id: String,
    pub name: String,
    #[allow(unused)]
    pub location: String,
    #[allow(unused)]
    pub col_type: String,
}

struct FormatConfig {
    k_sep: bool,
    precision: u32,
    precision_type: String, // "decimalDigits" or "significantDecimal"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormatLabel {
    Bi,
    Dsp1,
    Dsp2,
}

impl DataFormatLabel {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Bi => "BI",
            Self::Dsp1 => "DSP-1",
            Self::Dsp2 => "DSP-2",
        }
    }
}

fn detect_format(root: &Value) -> Option<DataFormatLabel> {
    let data = root.get("data")?;
    if data.get("vizData").is_some() {
        Some(DataFormatLabel::Bi)
    } else if data.get("resultList").is_some() {
        Some(DataFormatLabel::Dsp1)
    } else if data.get("dataList").is_some() {
        Some(DataFormatLabel::Dsp2)
    } else {
        None
    }
}

pub fn parse(json_str: &str, force_k_sep: Option<bool>) -> Result<TableData, String> {
    let root: Value =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {}", e))?;

    match detect_format(&root) {
        Some(DataFormatLabel::Bi) => parse_bi(&root, force_k_sep),
        Some(DataFormatLabel::Dsp1) => parse_dsp1(&root, force_k_sep),
        Some(DataFormatLabel::Dsp2) => parse_dsp2(&root),
        None => Err("Unknown JSON format: no vizData, resultList, or dataList found".into()),
    }
}

// ======================== BI parser ========================

fn parse_bi(root: &Value, force_k_sep: Option<bool>) -> Result<TableData, String> {
    let viz = root
        .get("data")
        .and_then(|d| d.get("vizData"))
        .ok_or("Missing data.vizData in JSON")?;

    let field_map = viz
        .get("fieldMap")
        .and_then(|f| f.as_object())
        .ok_or("Missing data.vizData.fieldMap")?;

    let location_map = viz
        .get("locationMap")
        .ok_or("Missing data.vizData.locationMap")?;

    // Build ordered column list: dimensions first, then measures
    let dims: &[Value] = location_map
        .get("dimensions")
        .and_then(|v| v.as_array())
        .map_or(&[], |v| v.as_slice());

    let measures: &[Value] = location_map
        .get("measures")
        .and_then(|v| v.as_array())
        .map_or(&[], |v| v.as_slice());

    let mut columns = Vec::new();
    let mut col_ids = Vec::new();

    for id_val in dims.iter().chain(measures.iter()) {
        let id = id_val
            .as_str()
            .ok_or("Invalid column id in locationMap")?
            .to_string();

        let field = field_map
            .get(&id)
            .ok_or_else(|| format!("Field {} not found in fieldMap", id))?;

        let name = field
            .get("alias")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();

        let col_type = field
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("string")
            .to_string();

        let location = field
            .get("location")
            .and_then(|v| v.as_str())
            .unwrap_or("measures")
            .to_string();

        columns.push(ColumnInfo {
            id: id.clone(),
            name,
            location,
            col_type,
        });
        col_ids.push(id);
    }

    // Parse rows
    let datasets = viz
        .get("datasets")
        .and_then(|v| v.as_array())
        .ok_or("Missing data.vizData.datasets")?;

    let mut rows = Vec::new();
    for row_obj in datasets {
        let row_map = row_obj.as_object().ok_or("Invalid row in datasets")?;
        let mut row = Vec::new();
        for col_id in &col_ids {
            let value_str = match row_map.get(col_id) {
                None | Some(Value::Null) => String::new(),
                Some(v) => format_json_value(v, col_id, field_map, force_k_sep),
            };
            row.push(value_str);
        }
        rows.push(row);
    }

    Ok(TableData {
        source_format: DataFormatLabel::Bi,
        columns,
        rows,
    })
}

fn format_json_value(
    value: &Value,
    col_id: &str,
    field_map: &serde_json::Map<String, Value>,
    force_k_sep: Option<bool>,
) -> String {
    let field = field_map.get(col_id);
    let col_type = field
        .and_then(|f| f.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("string");
    let fmt = get_format_config(field);

    match value {
        Value::Number(n) => format_number(n, &fmt, force_k_sep),
        Value::String(s) => {
            // Datasets often store numbers as strings — parse and format them
            if (col_type == "float" || col_type == "int")
                && !s.is_empty()
                && let Ok(n) = serde_json::from_str::<serde_json::Number>(s)
            {
                return format_number(&n, &fmt, force_k_sep);
            }
            s.clone()
        }
        Value::Bool(b) => b.to_string(),
        _ => value.to_string(),
    }
}

fn get_format_config(field: Option<&Value>) -> FormatConfig {
    let default = FormatConfig {
        k_sep: false,
        precision: 2,
        precision_type: "decimalDigits".to_string(),
    };

    let field = match field {
        Some(f) => f,
        None => return default,
    };

    // Try field.format first, then field.autoFormat.field
    let fmt = field
        .get("format")
        .or_else(|| field.get("autoFormat").and_then(|a| a.get("field")));

    let fmt = match fmt {
        Some(f) => f,
        None => return default,
    };

    FormatConfig {
        k_sep: fmt.get("kSep").and_then(|v| v.as_bool()).unwrap_or(false),
        precision: fmt.get("precision").and_then(|v| v.as_u64()).unwrap_or(2) as u32,
        precision_type: fmt
            .get("precisionType")
            .and_then(|v| v.as_str())
            .unwrap_or("decimalDigits")
            .to_string(),
    }
}

// ======================== DSP-1 parser (resultList) ========================

fn parse_dsp1(root: &Value, force_k_sep: Option<bool>) -> Result<TableData, String> {
    let result_list = root
        .get("data")
        .and_then(|d| d.get("resultList"))
        .and_then(|v| v.as_array())
        .ok_or("Missing data.resultList")?;

    // Extract column names from first row's keys (preserve insertion order)
    let columns = match result_list.first().and_then(|r| r.as_object()) {
        Some(first_row) => {
            let mut cols = Vec::new();
            for (key, _) in first_row {
                cols.push(ColumnInfo {
                    id: key.clone(),
                    name: key.clone(),
                    location: String::new(),
                    col_type: String::new(),
                });
            }
            cols
        }
        None => return Err("resultList is empty".into()),
    };

    let mut rows = Vec::new();
    for row_obj in result_list {
        let row_map = row_obj.as_object().ok_or("Invalid row in resultList")?;
        let mut row = Vec::new();
        for col in &columns {
            let value_str = match row_map.get(&col.id) {
                None | Some(Value::Null) => String::new(),
                Some(v) => format_dsp_value(v, force_k_sep),
            };
            row.push(value_str);
        }
        rows.push(row);
    }

    Ok(TableData {
        source_format: DataFormatLabel::Dsp1,
        columns,
        rows,
    })
}

// ======================== DSP-2 parser (dataList + columnList) ========================

fn parse_dsp2(root: &Value) -> Result<TableData, String> {
    let data = root.get("data").ok_or("Missing data")?;

    let column_list = data
        .get("columnList")
        .and_then(|v| v.as_array())
        .ok_or("Missing data.columnList")?;

    let columns: Vec<ColumnInfo> = column_list
        .iter()
        .map(|v| {
            let name = v.as_str().unwrap_or("").to_string();
            ColumnInfo {
                id: name.clone(),
                name,
                location: String::new(),
                col_type: String::new(),
            }
        })
        .collect();

    let data_list = data
        .get("dataList")
        .and_then(|v| v.as_array())
        .ok_or("Missing data.dataList")?;

    let mut rows = Vec::new();
    for row_obj in data_list {
        let row_map = row_obj.as_object().ok_or("Invalid row in dataList")?;
        let mut row = Vec::new();
        for col in &columns {
            let value_str = match row_map.get(&col.id) {
                None | Some(Value::Null) => String::new(),
                Some(v) => v.as_str().unwrap_or("").to_string(),
            };
            row.push(value_str);
        }
        rows.push(row);
    }

    Ok(TableData {
        source_format: DataFormatLabel::Dsp2,
        columns,
        rows,
    })
}

fn format_dsp_value(value: &Value, force_k_sep: Option<bool>) -> String {
    match value {
        Value::Number(n) => {
            if force_k_sep == Some(true) {
                if let Some(i) = n.as_i64() {
                    return format_int_with_commas(i);
                }
                if let Some(f) = n.as_f64() {
                    return format_float_with_sep(f);
                }
            }
            n.to_string()
        }
        Value::String(s) => {
            if force_k_sep == Some(true) && !s.is_empty() {
                if let Ok(i) = s.parse::<i64>() {
                    return format_int_with_commas(i);
                }
                if let Ok(f) = s.parse::<f64>() {
                    return format_float_with_sep(f);
                }
            }
            s.clone()
        }
        Value::Bool(b) => b.to_string(),
        _ => value.to_string(),
    }
}

fn format_float_with_sep(value: f64) -> String {
    let abs = value.abs();
    let int_part = abs.trunc() as i64;
    let frac = abs.fract();

    let sign = if value < 0.0 { "-" } else { "" };
    let int_str = format_int_with_commas(int_part);

    // Determine decimal places from the original value string representation
    let s = format!("{:.10}", frac);
    let s = s.trim_start_matches("0.");
    let s = s.trim_end_matches('0');

    if s.is_empty() {
        format!("{}{}", sign, int_str)
    } else {
        format!("{}{}.{}", sign, int_str, s)
    }
}

// ======================== Shared number formatting ========================

fn format_number(n: &serde_json::Number, fmt: &FormatConfig, force_k_sep: Option<bool>) -> String {
    let k_sep = force_k_sep.unwrap_or(fmt.k_sep);

    if let Some(i) = n.as_i64() {
        return if k_sep {
            format_with_separator(i as f64, 0)
        } else {
            i.to_string()
        };
    }

    let f = n.as_f64().unwrap_or(0.0);

    if fmt.precision_type == "significantDecimal" {
        format_significant(f, fmt.precision, k_sep)
    } else {
        // decimalDigits: fixed decimal places
        let rounded = round_to(f, fmt.precision);
        if k_sep {
            format_with_separator(rounded, fmt.precision)
        } else {
            format!("{:.prec$}", rounded, prec = fmt.precision as usize)
        }
    }
}

fn round_to(value: f64, precision: u32) -> f64 {
    let factor = 10_f64.powi(precision as i32);
    (value * factor).round() / factor
}

fn format_significant(value: f64, sig_digits: u32, k_sep: bool) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    let abs = value.abs();
    let magnitude = abs.log10().floor();
    let decimal_places = (sig_digits as i32 - magnitude as i32 - 1).max(0) as u32;

    let rounded = round_to(value, decimal_places);

    if k_sep {
        format_with_separator(rounded, decimal_places)
    } else {
        format!("{:.prec$}", rounded, prec = decimal_places as usize)
    }
}

fn format_with_separator(value: f64, decimal_places: u32) -> String {
    let abs_value = value.abs();
    let int_part = abs_value.trunc() as i64;
    let frac_part = abs_value.fract();

    // Format integer part with commas
    let int_str = format_int_with_commas(int_part);

    let sign = if value < 0.0 { "-" } else { "" };

    if decimal_places == 0 {
        format!("{}{}", sign, int_str)
    } else {
        format!(
            "{}{}.{:0>width$}",
            sign,
            int_str,
            (frac_part * 10_f64.powi(decimal_places as i32)).round() as i64,
            width = decimal_places as usize
        )
    }
}

fn format_int_with_commas(n: i64) -> String {
    let s = n.to_string();
    let (sign, digits) = s
        .strip_prefix('-')
        .map_or(("", s.as_str()), |rest| ("-", rest));
    let len = digits.len();
    if len <= 3 {
        return s;
    }
    let mut result = String::from(sign);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_int_with_commas() {
        assert_eq!(format_int_with_commas(0), "0");
        assert_eq!(format_int_with_commas(100), "100");
        assert_eq!(format_int_with_commas(1000), "1,000");
        assert_eq!(format_int_with_commas(45516565913), "45,516,565,913");
        assert_eq!(format_int_with_commas(-1000000), "-1,000,000");
    }

    #[test]
    fn test_parse_bi_1() {
        let json = std::fs::read_to_string("payload-bi-1.json").unwrap();
        let result = parse(&json, None).unwrap();
        assert_eq!(result.source_format, DataFormatLabel::Bi);
        assert_eq!(result.columns.len(), 6);
        assert_eq!(result.rows.len(), 3);

        assert_eq!(result.columns[0].name, "业务日期");
        assert_eq!(result.columns[2].name, "VN");

        let vn = &result.rows[0][2];
        assert!(vn.contains(","), "VN should have thousands separator: {vn}");
    }

    #[test]
    fn test_parse_bi_2_no_dimensions() {
        let json = std::fs::read_to_string("payload-bi-2.json").unwrap();
        let result = parse(&json, None).unwrap();
        assert_eq!(result.source_format.display_name(), "BI");
        assert_eq!(result.columns.len(), 13);
        assert_eq!(result.rows.len(), 18);

        assert_eq!(result.columns[0].name, "统一社会信用代码");
        assert_eq!(result.rows[0][5], "1.500");
        assert!(!result.rows[0][1].is_empty());
    }

    #[test]
    fn test_parse_dsp_1() {
        let json = std::fs::read_to_string("payload-dsp-1.json").unwrap();
        // The file is truncated in context, read it properly
        let result = parse(&json, None).unwrap();
        assert_eq!(result.source_format, DataFormatLabel::Dsp1);

        // DSP-1: columns are the keys of each object in resultList
        assert!(
            result.columns.len() > 10,
            "expected many columns, got {}",
            result.columns.len()
        );
        assert_eq!(result.columns[0].name, "企业名称（工商）");

        // Should have rows
        assert!(result.rows.len() >= 3);
    }

    #[test]
    fn test_parse_dsp_2() {
        let json = std::fs::read_to_string("payload-dsp-2.json").unwrap();
        let result = parse(&json, None).unwrap();
        assert_eq!(result.source_format, DataFormatLabel::Dsp2);

        assert_eq!(result.columns.len(), 3);
        assert_eq!(result.columns[0].name, "数据日期");
        assert_eq!(result.columns[1].name, "组源标志");
        assert_eq!(result.columns[2].name, "统一社会信用代码计数");

        // 100 rows
        assert!(result.rows.len() >= 90);
    }
}
