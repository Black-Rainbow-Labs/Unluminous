//! Records of an `.xlsb` part: a variable length type, a variable length size, and the bytes.
//!
//! A record type is one byte, or two when the high bit of the first is set (seven bits from each). A size
//! is up to four bytes of seven bits each, again with the high bit saying another follows.

pub use crate::xls::records::Reader;

/// One record of a binary part.
#[derive(Debug, Clone)]
pub struct Record<'a> {
    pub id: u16,
    pub data: &'a [u8],
}

/// Every record of a part, in order. A record cut short by the end of the part ends the list.
pub fn split(bytes: &[u8]) -> Vec<Record<'_>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let first = bytes[at];
        at += 1;
        let mut id = (first & 0x7F) as u16;
        if first & 0x80 != 0 {
            let Some(&second) = bytes.get(at) else { break };
            at += 1;
            id |= ((second & 0x7F) as u16) << 7;
        }
        let mut size = 0usize;
        let mut shift = 0;
        loop {
            let Some(&b) = bytes.get(at) else { return out };
            at += 1;
            size |= ((b & 0x7F) as usize) << shift;
            shift += 7;
            if b & 0x80 == 0 || shift > 28 {
                break;
            }
        }
        let Some(data) = bytes.get(at..at + size) else { break };
        out.push(Record { id, data });
        at += size;
    }
    out
}

/// Read up to `count` items, stopping at the first that cannot be read. A count in a damaged file can be
/// billions, and the items it counts cannot be more than the bytes there are.
pub fn many<T>(
    count: u32,
    r: &mut Reader,
    mut item: impl FnMut(&mut Reader) -> Option<T>,
) -> Vec<T> {
    let mut out = Vec::new();
    for _ in 0..count {
        match item(r) {
            Some(value) => out.push(value),
            None => break,
        }
    }
    out
}

/// A string with a 32 bit length in characters, then UTF-16.
pub fn wide_string(r: &mut Reader) -> Option<String> {
    let cch = r.u32()? as usize;
    r.chars(cch, true)
}

/// A string that may be absent: a length of all ones is no string.
pub fn nullable_string(r: &mut Reader) -> Option<Option<String>> {
    let cch = r.u32()?;
    if cch == 0xFFFF_FFFF {
        return Some(None);
    }
    Some(Some(r.chars(cch as usize, true)?))
}

/// The record types, by the names the file format specification gives them.
pub mod id {
    pub const ROW_HDR: u16 = 0;
    pub const CELL_BLANK: u16 = 1;
    pub const CELL_RK: u16 = 2;
    pub const CELL_ERROR: u16 = 3;
    pub const CELL_BOOL: u16 = 4;
    pub const CELL_REAL: u16 = 5;
    pub const CELL_ST: u16 = 6;
    pub const CELL_ISST: u16 = 7;
    pub const FMLA_STRING: u16 = 8;
    pub const FMLA_NUM: u16 = 9;
    pub const FMLA_BOOL: u16 = 10;
    pub const FMLA_ERROR: u16 = 11;
    pub const SST_ITEM: u16 = 19;
    pub const NAME: u16 = 39;
    pub const FONT: u16 = 43;
    pub const FMT: u16 = 44;
    pub const FILL: u16 = 45;
    pub const BORDER: u16 = 46;
    pub const XF: u16 = 47;
    pub const COL_INFO: u16 = 60;
    pub const DVAL: u16 = 64;
    pub const FILE_VERSION: u16 = 128;
    pub const BEGIN_SHEET: u16 = 129;
    pub const BEGIN_WS_VIEW: u16 = 137;
    pub const BEGIN_SHEET_DATA: u16 = 145;
    pub const END_SHEET_DATA: u16 = 146;
    pub const WS_PROP: u16 = 147;
    pub const WS_DIM: u16 = 148;
    pub const PANE: u16 = 151;
    pub const WB_PROP: u16 = 153;
    pub const BUNDLE_SH: u16 = 156;
    pub const CALC_PROP: u16 = 157;
    pub const BOOK_VIEW: u16 = 158;
    pub const BEGIN_SST: u16 = 159;
    pub const BEGIN_AFILTER: u16 = 161;
    pub const BEGIN_FILTER_COLUMN: u16 = 163;
    pub const END_FILTER_COLUMN: u16 = 164;
    pub const BEGIN_FILTERS: u16 = 165;
    pub const FILTER: u16 = 167;
    pub const COLOR_FILTER: u16 = 168;
    pub const TOP10_FILTER: u16 = 170;
    pub const DYNAMIC_FILTER: u16 = 171;
    pub const BEGIN_CUSTOM_FILTERS: u16 = 172;
    pub const CUSTOM_FILTER: u16 = 174;
    pub const MERGE_CELL: u16 = 176;
    pub const BEGIN_CF: u16 = 461;
    pub const BEGIN_CF_RULE: u16 = 463;
    pub const END_CF: u16 = 462;
    pub const END_CF_RULE: u16 = 464;
    pub const BEGIN_ICON_SET: u16 = 465;
    pub const END_ICON_SET: u16 = 466;
    pub const BEGIN_DATABAR: u16 = 467;
    pub const END_DATABAR: u16 = 468;
    pub const BEGIN_COLOR_SCALE: u16 = 469;
    pub const END_COLOR_SCALE: u16 = 470;
    pub const CFVO: u16 = 471;
    pub const COLOR: u16 = 564;
    pub const WS_FMT_INFO: u16 = 485;
    pub const HLINK: u16 = 494;
    pub const SHEET_PROTECTION: u16 = 535;
    pub const BEGIN_FMTS: u16 = 615;
    pub const BEGIN_FONTS: u16 = 611;
    pub const BEGIN_FILLS: u16 = 603;
    pub const BEGIN_BORDERS: u16 = 613;
    pub const BEGIN_CELL_XFS: u16 = 617;
    pub const BEGIN_CELL_STYLE_XFS: u16 = 626;
    pub const BEGIN_DXFS: u16 = 505;
    pub const DXF: u16 = 507;
    pub const END_DXFS: u16 = 506;
    pub const SHR_FMLA: u16 = 428;
    pub const ARR_FMLA: u16 = 426;
    pub const BEGIN_COMMENTS: u16 = 628;
    pub const COMMENT_AUTHOR: u16 = 632;
    pub const BEGIN_COMMENT: u16 = 635;
    pub const COMMENT_TEXT: u16 = 637;
    pub const EXTERN_SHEET: u16 = 362;
    pub const SUP_SELF: u16 = 357;
    pub const SUP_SAME: u16 = 358;
    pub const SUP_BOOK_SRC: u16 = 355;
    pub const SUP_ADDIN: u16 = 360;
    pub const SUP_TABS: u16 = 359;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_sizes_of_one_and_two_bytes_are_read() {
        // Type 5 size 2; type 0x0267 (two bytes) size 130 (two bytes).
        let mut bytes = vec![0x05, 0x02, 0xAA, 0xBB, 0xE7, 0x04, 0x82, 0x01];
        bytes.extend(vec![0u8; 130]);
        let records = split(&bytes);
        assert_eq!(records.len(), 2);
        assert_eq!((records[0].id, records[0].data), (5, &[0xAA, 0xBB][..]));
        assert_eq!((records[1].id, records[1].data.len()), (615, 130));
    }
}
