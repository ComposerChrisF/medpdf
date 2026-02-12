# pdf-merger

A command-line tool for advanced PDF manipulation: merge pages from multiple PDFs, apply overlays, add watermarks, and pad to page multiples.

## Installation

```bash
cargo build --release -p pdf-merger
```

The binary will be at `target/release/pdf-merger`.

## Usage

```bash
pdf-merger -o <OUTPUT> <FILE> <PAGES> [<FILE> <PAGES>]... [OPTIONS]
```

### Basic Merging

Input files and page specifications come in pairs:

```bash
# Merge all pages from two PDFs
pdf-merger -o combined.pdf doc1.pdf "all" doc2.pdf "all"

# Merge specific pages
pdf-merger -o output.pdf report.pdf "1-5" appendix.pdf "2,4,6"
```

### Page Specifications

| Format | Description | Example |
|--------|-------------|---------|
| `all` | All pages | `"all"` |
| `N` | Single page | `"3"` |
| `N-M` | Page range (inclusive) | `"1-5"` |
| `N-` | From page N to end | `"10-"` |
| `-M` | From start to page M | `"-5"` |
| `N,M,P` | Specific pages | `"1,3,7"` |
| Mixed | Combine formats | `"1-3,5,8-10"` |

## Options

### Output

| Option | Description |
|--------|-------------|
| `-o, --output <FILE>` | Output PDF path (required) |
| `--broad-compatibility` | Use traditional PDF format for older viewers |

### Watermarks

Add text watermarks on top of page content:

```bash
--watermark "text=DRAFT,font=@Helvetica,size=48,x=1,y=1,units=in,pages=all"
```

Add watermarks behind page content:

```bash
--watermark-under "text=CONFIDENTIAL,font=@Courier,size=36,x=0.5,y=0.5,units=in"
```

**Watermark parameters:**

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `text` | Yes | - | Watermark text |
| `font` | Yes | - | Font specification (see below) |
| `size` | No | 48 | Font size in points |
| `x` | Yes | - | X position |
| `y` | Yes | - | Y position |
| `units` | No | `in` | Position units: `in` (inches) or `mm` (millimeters) |
| `pages` | No | `all` | Pages to watermark (same format as page specs) |

**Font specifications:**

- `@Helvetica` - PDF built-in font (prefix with `@`)
- `@Courier`, `@Times-Roman`, `@Symbol`, `@ZapfDingbats` - Other built-ins
- `Arial` - System font name (searched via font-kit)
- `/path/to/font.ttf` - Direct path to TTF file

Built-in fonts (PDF 1.7): `Times-Roman`, `Helvetica`, `Courier`, `Symbol`, `Times-Bold`, `Helvetica-Bold`, `Courier-Bold`, `ZapfDingbats`, `Times-Italic`, `Helvetica-Oblique`, `Courier-Oblique`, `Times-BoldItalic`, `Helvetica-BoldOblique`, `Courier-BoldOblique`

### Overlays

Overlay content from another PDF onto pages:

```bash
--overlay "file=letterhead.pdf,src_page=1,target_pages=all"
```

**Overlay parameters:**

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `file` | Yes | - | Source PDF file |
| `src_page` | Yes | - | Page number from source PDF to overlay |
| `target_pages` | No | `all` | Destination pages to apply overlay |

### Padding

Pad the document to a multiple of N pages (useful for booklet printing):

```bash
--pad-to 4
```

Optionally use a specific page for the last padding page:

```bash
--pad-to 4 --pad-last-page-file "file=back-cover.pdf,page=1"
```

**Pad file parameters:**

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `file` | Yes | - | PDF file for last padding page |
| `page` | No | 1 | Page number to use from file |

### Encryption

```bash
--user-password "viewpassword"
--owner-password "editpassword"
```

## Examples

### Merge with page selection

```bash
pdf-merger -o report.pdf \
  cover.pdf "1" \
  content.pdf "all" \
  appendix.pdf "1-3,7"
```

### Add watermark to all pages

```bash
pdf-merger -o draft.pdf document.pdf "all" \
  --watermark "text=DRAFT,font=@Helvetica-Bold,size=72,x=2,y=5,units=in"
```

### Apply letterhead overlay

```bash
pdf-merger -o branded.pdf document.pdf "all" \
  --overlay "file=letterhead.pdf,src_page=1,target_pages=all"
```

### Prepare for booklet printing (4-page signatures)

```bash
pdf-merger -o booklet.pdf document.pdf "all" --pad-to 4
```

### Complex workflow

```bash
pdf-merger -o final.pdf \
  intro.pdf "1-2" \
  main.pdf "all" \
  appendix.pdf "5-" \
  --overlay "file=template.pdf,src_page=1,target_pages=1" \
  --watermark "text=v1.0,font=@Courier,size=12,x=0.5,y=0.25,units=in,pages=1" \
  --watermark-under "text=CONFIDENTIAL,font=@Helvetica,size=48,x=3,y=5,units=in,pages=2-" \
  --pad-to 4
```

## Processing Pipeline

The tool processes PDFs in five phases:

1. **Merge** - Copy selected pages from input files
2. **Overlay** - Apply PDF overlays with resource deduplication
3. **Watermark** - Add text watermarks (under first, then over)
4. **Pad** - Add blank pages to reach target multiple
5. **Save** - Compress and write output

## License

See repository root for license information.
