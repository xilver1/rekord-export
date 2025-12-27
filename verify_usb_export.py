#!/usr/bin/env python3
"""Complete rekordbox USB export analyzer"""
import struct
import sys
import os

def analyze_devsetting(filepath):
    """Analyze DEVSETTING.DAT"""
    print(f"\n{'='*60}")
    print(f"DEVSETTING.DAT Analysis")
    print(f"{'='*60}")
    
    with open(filepath, 'rb') as f:
        data = f.read()
    
    print(f"File size: {len(data)} bytes (expected: 140)")
    
    if len(data) < 140:
        print("❌ ERROR: File too small!")
        return False
    
    # Parse fields
    size = struct.unpack('<I', data[0:4])[0]
    brand = data[4:32].rstrip(b'\x00').decode('ascii', errors='replace')
    app = data[36:68].rstrip(b'\x00').decode('ascii', errors='replace')
    version = data[68:100].rstrip(b'\x00').decode('ascii', errors='replace')
    marker = struct.unpack('<I', data[100:104])[0]
    magic = struct.unpack('<I', data[104:108])[0]
    
    print(f"  Size header: 0x{size:X} (expected 0x60)")
    print(f"  Brand: '{brand}' (expected 'PIONEER DJ')")
    print(f"  Application: '{app}' (expected 'rekordbox')")
    print(f"  Version: '{version}'")
    print(f"  Marker: 0x{marker:X} (expected 0x20)")
    print(f"  Magic: 0x{magic:X} (expected 0x12345678)")
    
    valid = (size == 0x60 and 'PIONEER' in brand and 
             'rekordbox' in app and magic == 0x12345678)
    print(f"\n{'✅ VALID' if valid else '❌ INVALID'}")
    return valid

def analyze_djprofile(filepath):
    """Analyze djprofile.nxs"""
    print(f"\n{'='*60}")
    print(f"djprofile.nxs Analysis")
    print(f"{'='*60}")
    
    with open(filepath, 'rb') as f:
        data = f.read()
    
    print(f"File size: {len(data)} bytes (expected: 160)")
    
    if len(data) < 160:
        print("❌ ERROR: File too small!")
        return False
    
    # Profile name at offset 0x20
    name = data[0x20:0x40].rstrip(b'\x00').decode('ascii', errors='replace')
    print(f"  DJ Profile name: '{name}'")
    
    # Check padding
    padding_ok = all(b == 0 for b in data[0:0x20]) and all(b == 0 for b in data[0x40:])
    print(f"  Padding: {'✅ OK' if padding_ok else '⚠️ Non-zero padding'}")
    
    print(f"\n✅ VALID")
    return True

def analyze_pdb(filepath):
    """Analyze export.pdb"""
    print(f"\n{'='*60}")
    print(f"export.pdb Analysis")
    print(f"{'='*60}")
    
    with open(filepath, 'rb') as f:
        data = f.read()
    
    print(f"File size: {len(data)} bytes ({len(data)//4096} pages)")
    
    # File header
    unknown1, page_size, num_tables, next_unused = struct.unpack_from('<IIII', data, 0)
    unknown2, sequence, unknown3 = struct.unpack_from('<III', data, 16)
    
    print(f"\nHeader:")
    print(f"  Page size: {page_size} {'✅' if page_size == 4096 else '❌'}")
    print(f"  Number of tables: {num_tables}")
    print(f"  Next unused page: {next_unused}")
    print(f"  Sequence: {sequence}")
    
    table_names = {
        0: "Tracks", 1: "Genres", 2: "Artists", 3: "Albums",
        4: "Labels", 5: "Keys", 6: "Colors", 7: "PlaylistTree",
        8: "PlaylistEntries", 13: "Artwork", 16: "Columns",
        17: "HistoryPlaylists", 18: "HistoryEntries", 19: "History"
    }
    
    required_tables = {0, 1, 2, 3, 6}  # Tracks, Genres, Artists, Albums, Colors
    found_tables = set()
    
    print(f"\nTables:")
    offset = 28
    for i in range(num_tables):
        if offset + 16 > len(data):
            break
        table_type, empty_candidate, first_page, last_page = struct.unpack_from('<IIII', data, offset)
        name = table_names.get(table_type, f"Unknown{table_type}")
        found_tables.add(table_type)
        
        # Count rows in this table
        total_rows = 0
        for page_num in range(first_page, last_page + 1):
            if page_num == 0 or page_num * 4096 + 28 > len(data):
                continue
            page_offset = page_num * 4096
            row_data = struct.unpack_from('<I', data, page_offset + 24)[0]
            num_rows = (row_data >> 13) & 0x7FF
            total_rows += num_rows
        
        print(f"  Type {table_type:2d} ({name:16s}): pages {first_page}-{last_page}, ~{total_rows} rows")
        offset += 16
    
    # Check required tables
    missing = required_tables - found_tables
    if missing:
        print(f"\n❌ Missing required tables: {[table_names.get(t, t) for t in missing]}")
        return False
    
    print(f"\n✅ All required tables present")
    return True

def analyze_anlz(filepath):
    """Analyze ANLZ file (.DAT, .EXT, or .2EX)"""
    filename = os.path.basename(filepath)
    print(f"\n{'='*60}")
    print(f"{filename} Analysis")
    print(f"{'='*60}")
    
    with open(filepath, 'rb') as f:
        data = f.read()
    
    print(f"File size: {len(data)} bytes")
    
    if len(data) < 28:
        print("❌ ERROR: File too small!")
        return False
    
    # PMAI header
    tag = data[0:4].decode('ascii', errors='replace')
    if tag != "PMAI":
        print(f"❌ ERROR: Invalid header tag '{tag}' (expected 'PMAI')")
        return False
    
    header_len = struct.unpack('>I', data[4:8])[0]
    total_len = struct.unpack('>I', data[8:12])[0]
    
    print(f"  PMAI header_len={header_len}, declared_size={total_len}")
    if total_len != len(data):
        print(f"  ⚠️ Size mismatch: declared {total_len} vs actual {len(data)}")
    
    # Parse sections
    sections = {}
    offset = 4 + header_len  # Skip PMAI header
    
    while offset < len(data) - 12:
        tag = data[offset:offset+4]
        if tag[0] == 0:
            break
        tag_str = tag.decode('ascii', errors='replace')
        
        header_len = struct.unpack('>I', data[offset+4:offset+8])[0]
        section_len = struct.unpack('>I', data[offset+8:offset+12])[0]
        
        if section_len == 0 or offset + section_len > len(data):
            break
        
        info = ""
        if tag_str == "PPTH":
            path_len = struct.unpack('>I', data[offset+12:offset+16])[0]
            try:
                path = data[offset+16:offset+16+path_len*2].decode('utf-16-be')
                info = f"path='{path[:40]}{'...' if len(path)>40 else ''}'"
            except:
                info = f"path_len={path_len}"
        elif tag_str == "PQTZ":
            beat_count = struct.unpack('>I', data[offset+20:offset+24])[0]
            info = f"beats={beat_count}"
        elif tag_str == "PWAV":
            entry_count = struct.unpack('>I', data[offset+12:offset+16])[0]
            info = f"entries={entry_count} {'✅' if entry_count == 400 else '⚠️'}"
        elif tag_str == "PWV3":
            entry_count = struct.unpack('>I', data[offset+12:offset+16])[0]
            info = f"entries={entry_count}"
        elif tag_str == "PWV4":
            entry_count = struct.unpack('>I', data[offset+12:offset+16])[0]
            expected_size = 20 + 1200 * 6
            info = f"entries={entry_count} {'✅' if entry_count == 1200 else '⚠️'}"
        elif tag_str == "PWV5":
            entry_count = struct.unpack('>I', data[offset+12:offset+16])[0]
            info = f"entries={entry_count}"
        elif tag_str in ("PCOB", "PCO2"):
            cue_type = struct.unpack('>I', data[offset+12:offset+16])[0]
            cue_count = struct.unpack('>H', data[offset+18:offset+20])[0]
            info = f"type={'hot' if cue_type else 'memory'}, count={cue_count}"
        
        sections[tag_str] = section_len
        print(f"  {tag_str}: size={section_len:6d} {info}")
        
        offset += section_len
    
    # Check required sections based on file type
    ext = os.path.splitext(filepath)[1].upper()
    if ext == ".DAT":
        required = {"PPTH", "PQTZ", "PWAV", "PWV5"}
    else:  # .EXT or .2EX
        required = {"PPTH", "PQTZ", "PWAV", "PWV3", "PWV4", "PWV5"}
    
    missing = required - set(sections.keys())
    if missing:
        print(f"\n❌ Missing sections: {missing}")
        return False
    
    print(f"\n✅ All required sections present")
    return True

def main():
    usb_root = sys.argv[1] if len(sys.argv) > 1 else "example-USB"
    
    print("=" * 60)
    print("REKORDBOX USB EXPORT VERIFICATION")
    print("=" * 60)
    print(f"Analyzing: {usb_root}")
    
    results = {}
    
    # Check directory structure
    print(f"\n{'='*60}")
    print("Directory Structure Check")
    print(f"{'='*60}")
    
    required_paths = [
        "PIONEER",
        "PIONEER/rekordbox",
        "PIONEER/rekordbox/export.pdb",
        "PIONEER/USBANLZ",
        "Contents"
    ]
    
    for path in required_paths:
        full = os.path.join(usb_root, path)
        exists = os.path.exists(full)
        print(f"  {path}: {'✅' if exists else '❌'}")
    
    # Analyze individual files
    devsetting = os.path.join(usb_root, "PIONEER/DEVSETTING.DAT")
    if os.path.exists(devsetting):
        results['DEVSETTING'] = analyze_devsetting(devsetting)
    
    djprofile = os.path.join(usb_root, "PIONEER/djprofile.nxs")
    if os.path.exists(djprofile):
        results['djprofile'] = analyze_djprofile(djprofile)
    
    pdb = os.path.join(usb_root, "PIONEER/rekordbox/export.pdb")
    if os.path.exists(pdb):
        results['export.pdb'] = analyze_pdb(pdb)
    
    # Find and analyze ANLZ files
    anlz_dir = os.path.join(usb_root, "PIONEER/USBANLZ")
    if os.path.exists(anlz_dir):
        for root, dirs, files in os.walk(anlz_dir):
            for f in files:
                if f.endswith(('.DAT', '.EXT', '.2EX')):
                    path = os.path.join(root, f)
                    results[f] = analyze_anlz(path)
    
    # Summary
    print(f"\n{'='*60}")
    print("SUMMARY")
    print(f"{'='*60}")
    
    all_pass = all(results.values())
    for name, passed in results.items():
        print(f"  {name}: {'✅ PASS' if passed else '❌ FAIL'}")
    
    print(f"\n{'✅ USB EXPORT VALID' if all_pass else '❌ USB EXPORT HAS ISSUES'}")
    print()
    
    return 0 if all_pass else 1

if __name__ == "__main__":
    sys.exit(main())
