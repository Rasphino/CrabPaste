use serde_json::Value;

pub struct TableData {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<String>>,
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

pub fn parse(json_str: &str) -> Result<TableData, String> {
    let root: Value =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {}", e))?;

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
    let dims = location_map
        .get("dimensions")
        .and_then(|v| v.as_array())
        .ok_or("Missing locationMap.dimensions")?;

    let measures = location_map
        .get("measures")
        .and_then(|v| v.as_array())
        .ok_or("Missing locationMap.measures")?;

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
        let row_map = row_obj
            .as_object()
            .ok_or("Invalid row in datasets")?;
        let mut row = Vec::new();
        for col_id in &col_ids {
            let value_str = match row_map.get(col_id) {
                None | Some(Value::Null) => String::new(),
                Some(v) => format_json_value(v, col_id, field_map),
            };
            row.push(value_str);
        }
        rows.push(row);
    }

    Ok(TableData { columns, rows })
}

fn format_json_value(value: &Value, col_id: &str, field_map: &serde_json::Map<String, Value>) -> String {
    let field = field_map.get(col_id);
    let col_type = field
        .and_then(|f| f.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("string");
    let fmt = get_format_config(field);

    match value {
        Value::Number(n) => format_number(n, &fmt),
        Value::String(s) => {
            // Datasets often store numbers as strings — parse and format them
            if (col_type == "float" || col_type == "int") && !s.is_empty() {
                if let Ok(n) = serde_json::from_str::<serde_json::Number>(s) {
                    return format_number(&n, &fmt);
                }
            }
            s.clone()
        }
        Value::Bool(b) => b.to_string(),
        _ => value.to_string(),
    }
}

fn get_format_config(
    field: Option<&Value>,
) -> FormatConfig {
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
    let fmt = field.get("format").or_else(|| {
        field
            .get("autoFormat")
            .and_then(|a| a.get("field"))
    });

    let fmt = match fmt {
        Some(f) => f,
        None => return default,
    };

    FormatConfig {
        k_sep: fmt.get("kSep").and_then(|v| v.as_bool()).unwrap_or(false),
        precision: fmt
            .get("precision")
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as u32,
        precision_type: fmt
            .get("precisionType")
            .and_then(|v| v.as_str())
            .unwrap_or("decimalDigits")
            .to_string(),
    }
}

fn format_number(n: &serde_json::Number, fmt: &FormatConfig) -> String {
    if let Some(i) = n.as_i64() {
        return if fmt.k_sep {
            format_with_separator(i as f64, 0)
        } else {
            i.to_string()
        };
    }

    let f = n.as_f64().unwrap_or(0.0);

    if fmt.precision_type == "significantDecimal" {
        format_significant(f, fmt.precision, fmt.k_sep)
    } else {
        // decimalDigits: fixed decimal places
        let rounded = round_to(f, fmt.precision);
        if fmt.k_sep {
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
    let len = s.len();
    if len <= 3 {
        return s;
    }
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
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
    }

    #[test]
    fn test_parse_payload() {
        let json = std::fs::read_to_string("payload.json").unwrap();
        let result = parse(&json).unwrap();
        assert_eq!(result.columns.len(), 6);
        assert_eq!(result.rows.len(), 3);

        // Check column names
        assert_eq!(result.columns[0].name, "业务日期");
        assert_eq!(result.columns[2].name, "VN");

        // Check first row - VN should be formatted with commas
        let vn = &result.rows[0][2];
        assert!(vn.contains(","), "VN should have thousands separator: {vn}");
    }
}
