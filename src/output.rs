use clap::ValueEnum;
use serde_json::Value;

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
    Table,
}

pub fn render(value: &Value, format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => json(value),
        OutputFormat::Text => {
            let mut lines = Vec::new();
            text(value, &mut lines);
            lines.join("\n")
        }
        OutputFormat::Table => {
            let mut lines = Vec::new();
            table(value, None, &mut lines);
            lines.join("\n")
        }
    }
}

pub fn json(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("a JSON value serializes")
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::String(string) => string.clone(),
        other => other.to_string(),
    }
}

/// Scalars of an object or a list go on one tab-separated line; nested lists
/// and objects follow on their own lines.
fn text(value: &Value, lines: &mut Vec<String>) {
    let children: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) => map.values().collect(),
        other => {
            lines.push(scalar(other));
            return;
        }
    };
    let scalars: Vec<String> = children
        .iter()
        .filter(|child| is_scalar(child))
        .map(|child| scalar(child))
        .collect();
    if !scalars.is_empty() {
        lines.push(scalars.join("\t"));
    }
    for child in children.into_iter().filter(|child| !is_scalar(child)) {
        text(child, lines);
    }
}

fn width(cell: &str) -> usize {
    cell.chars().count()
}

fn grid(title: Option<&str>, header: Option<Vec<String>>, rows: Vec<Vec<String>>) -> Vec<String> {
    let columns = header
        .as_ref()
        .map(Vec::len)
        .into_iter()
        .chain(rows.iter().map(Vec::len))
        .max()
        .unwrap_or(0);
    let mut widths = vec![0; columns];
    for row in header.iter().chain(rows.iter()) {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(width(cell));
        }
    }
    let border = format!(
        "+{}+",
        widths
            .iter()
            .map(|width| "-".repeat(width + 2))
            .collect::<Vec<_>>()
            .join("+")
    );
    let line = |row: &Vec<String>| {
        let cells: Vec<String> = widths
            .iter()
            .enumerate()
            .map(|(index, width)| {
                let cell = row.get(index).map(String::as_str).unwrap_or("");
                format!(" {cell}{} ", " ".repeat(width - self::width(cell)))
            })
            .collect();
        format!("|{}|", cells.join("|"))
    };
    let mut lines = Vec::new();
    if let Some(title) = title {
        let inner = border.chars().count() - 2;
        let pad = inner.saturating_sub(width(title));
        lines.push(format!("+{}+", "-".repeat(inner.max(width(title)))));
        lines.push(format!(
            "|{}{title}{}|",
            " ".repeat(pad / 2),
            " ".repeat(pad - pad / 2)
        ));
    }
    lines.push(border.clone());
    if let Some(header) = &header {
        lines.push(line(header));
        lines.push(border.clone());
    }
    for row in &rows {
        lines.push(line(row));
    }
    lines.push(border);
    lines
}

/// A list of objects becomes one table with a column per key; an object
/// becomes a key-value table followed by a table per nested value.
fn table(value: &Value, title: Option<&str>, lines: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            let rows: Vec<Vec<String>> = map
                .iter()
                .filter(|(_, value)| is_scalar(value))
                .map(|(key, value)| vec![key.clone(), scalar(value)])
                .collect();
            if !rows.is_empty() {
                lines.extend(grid(title, None, rows));
            }
            for (key, value) in map.iter().filter(|(_, value)| !is_scalar(value)) {
                table(value, Some(key), lines);
            }
        }
        Value::Array(items) if items.iter().all(|item| item.is_object()) => {
            let mut header: Vec<String> = Vec::new();
            for item in items {
                for key in item.as_object().into_iter().flat_map(|map| map.keys()) {
                    if !header.contains(key) {
                        header.push(key.clone());
                    }
                }
            }
            let rows = items
                .iter()
                .map(|item| {
                    header
                        .iter()
                        .map(|key| item.get(key).map(scalar).unwrap_or_default())
                        .collect()
                })
                .collect();
            lines.extend(grid(title, Some(header), rows));
        }
        Value::Array(items) => {
            let rows = items.iter().map(|item| vec![scalar(item)]).collect();
            lines.extend(grid(title, None, rows));
        }
        other => lines.extend(grid(title, None, vec![vec![scalar(other)]])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_puts_scalars_on_one_line() {
        let value = json!({"SourceLanguageCode": null, "TargetLanguageCode": "en", "TranslatedText": "Hello"});
        assert_eq!(render(&value, OutputFormat::Text), "None\ten\tHello");
    }

    #[test]
    fn text_puts_each_list_item_on_its_own_line() {
        let value = json!({"Languages": [
            {"LanguageCode": "ja", "LanguageName": "Japanese"},
            {"LanguageCode": "en", "LanguageName": "English"},
        ]});
        assert_eq!(
            render(&value, OutputFormat::Text),
            "ja\tJapanese\nen\tEnglish"
        );
    }

    #[test]
    fn table_has_a_column_per_key() {
        let value = json!({"Languages": [{"LanguageCode": "ja", "LanguageName": "Japanese"}]});
        assert_eq!(
            render(&value, OutputFormat::Table),
            [
                "+-----------------------------+",
                "|          Languages          |",
                "+--------------+--------------+",
                "| LanguageCode | LanguageName |",
                "+--------------+--------------+",
                "| ja           | Japanese     |",
                "+--------------+--------------+",
            ]
            .join("\n")
        );
    }
}
