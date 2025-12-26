use async_graphql::dynamic::Schema;
use async_graphql::SDLExportOptions;

/// SDL returned from AsyncSchemaInner isn't standard
/// We clean it up before returning.
pub fn print_schema(schema: Schema) -> String {
    let sdl = schema.sdl_with_options(SDLExportOptions::new().sorted_fields());
    let lines: Vec<&str> = sdl.lines().collect();
    let mut result = String::new();
    let mut prev_line_empty = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed_line = line.trim();

        // Check if this is the start of a docstring block that precedes @include or
        // @skip
        if trimmed_line == r#"""""# {
            // Look ahead to find the end of the docstring and check what follows
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim().starts_with(r#"""""#) {
                j += 1;
            }
            // j now points to the closing """
            if j + 1 < lines.len() {
                let after_docstring = lines[j + 1].trim();
                if after_docstring.starts_with("directive @include")
                    || after_docstring.starts_with("directive @skip")
                {
                    // Skip the entire docstring and the directive
                    i = j + 2;
                    continue;
                }
            }
        }

        // Check if line contains the directives to be skipped
        if trimmed_line.starts_with("directive @include")
            || trimmed_line.starts_with("directive @skip")
        {
            i += 1;
            continue;
        }
        if trimmed_line.is_empty() {
            if !prev_line_empty {
                result.push('\n');
            }
            prev_line_empty = true;
        } else {
            let formatted_line = if line.starts_with('\t') {
                line.replace('\t', "  ")
            } else {
                line.to_string()
            };
            result.push_str(&formatted_line);
            result.push('\n');
            prev_line_empty = false;
        }
        i += 1;
    }

    result.trim().to_string()
}
