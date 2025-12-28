#!/usr/bin/env python3
"""Validate PDB file against rekordbox requirements."""

import struct
import sys

PAGE_SIZE = 4096

def validate_pdb(filepath):
    with open(filepath, 'rb') as f:
        data = f.read()
    
    print(f"Validating: {filepath}")
    print(f"File size: {len(data)} bytes ({len(data) // PAGE_SIZE} pages)")
    print()
    
    errors = []
    warnings = []
    
    # Check file header
    if struct.unpack('<I', data[4:8])[0] != PAGE_SIZE:
        errors.append("Invalid page size in header")
    
    num_tables = struct.unpack('<I', data[8:12])[0]
    if num_tables != 20:
        warnings.append(f"Expected 20 tables, found {num_tables}")
    
    # Check data pages
    for page_num in range(1, len(data) // PAGE_SIZE):
        page = data[page_num * PAGE_SIZE:(page_num + 1) * PAGE_SIZE]
        
        if all(b == 0 for b in page):
            continue
        
        flags = page[0x1B]
        is_data = (flags & 0x40) == 0
        
        if not is_data:
            continue
        
        # Parse row counts
        packed = page[0x18] | (page[0x19] << 8) | (page[0x1A] << 16)
        num_rows = packed & 0x7FF
        num_offsets = packed >> 11
        
        # Check 4:1 ratio
        if num_rows > 0:
            ratio = num_offsets / num_rows
            if ratio != 4.0:
                warnings.append(f"Page {page_num}: offset ratio {ratio:.1f} (expected 4.0)")
        
        # Check row group structure
        num_groups = (num_rows + 15) // 16 if num_rows > 0 else 1
        for g in range(num_groups):
            group_start = PAGE_SIZE - (g + 1) * 36
            presence = struct.unpack('<H', page[group_start + 32:group_start + 34])[0]
            pad = struct.unpack('<H', page[group_start + 34:group_start + 36])[0]
            
            if presence != pad:
                errors.append(f"Page {page_num} group {g}: presence ({presence:#x}) != pad ({pad:#x})")
    
    # Report
    if errors:
        print("ERRORS:")
        for e in errors:
            print(f"  ✗ {e}")
    
    if warnings:
        print("WARNINGS:")
        for w in warnings:
            print(f"  ⚠ {w}")
    
    if not errors and not warnings:
        print("✓ All checks passed")
    
    return len(errors) == 0

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <export.pdb>")
        sys.exit(1)
    
    success = validate_pdb(sys.argv[1])
    sys.exit(0 if success else 1)
