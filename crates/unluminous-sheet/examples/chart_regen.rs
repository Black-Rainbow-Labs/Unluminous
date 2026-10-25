//! Write copies of a workbook with some of its charts written from the model instead of kept, so a chart
//! that Excel does not accept after that can be found.
//!
//!   cargo run -p unluminous-sheet --example chart_regen -- <input.xlsx> <folder> [classic|ex|each]

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("an input file");
    let folder = std::path::PathBuf::from(args.next().expect("a folder"));
    let mode = args.next().unwrap_or_else(|| "classic".to_owned());
    std::fs::create_dir_all(&folder).unwrap();
    let book = unluminous_sheet::xlsx::read(&std::fs::read(&input).unwrap()).unwrap();
    let write = |name: &str, pick: &dyn Fn(usize, usize, &unluminous_sheet::model::Chart) -> bool| {
        let mut copy = book.clone();
        for (s, sheet) in copy.sheets.iter_mut().enumerate() {
            for (c, chart) in sheet.charts.iter_mut().enumerate() {
                chart.spec.dirty = pick(s, c, chart);
            }
        }
        std::fs::write(folder.join(name), unluminous_sheet::xlsx::write(&copy).unwrap()).unwrap();
    };
    match mode.as_str() {
        "classic" => write("classic.xlsx", &|_, _, c| c.spec.ex.is_none()),
        "ex" => write("ex.xlsx", &|_, _, c| c.spec.ex.is_some()),
        _ => {
            for (s, sheet) in book.sheets.iter().enumerate() {
                for (c, chart) in sheet.charts.iter().enumerate() {
                    let name = format!("one-{s}-{c:02}-{}.xlsx", chart.chart_type().name());
                    write(&name, &|ss, cc, _| ss == s && cc == c);
                }
            }
        }
    }
}
