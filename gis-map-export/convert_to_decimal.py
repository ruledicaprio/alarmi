import json, re, csv

def dms_to_decimal(dms_str):
    if not dms_str or dms_str == 'null': return None
    pattern = r'([EWNS])(\d+)°\s*(\d+)\'\s*([\d,]+)"?'
    m = re.match(pattern, str(dms_str).strip().replace('"', ''))
    if not m: return None
    dir, deg, min_, sec = m.groups()
    sec = float(sec.replace(',', '.'))
    dec = int(deg) + int(min_)/60 + sec/3600
    return round(-dec if dir in 'WS' else dec, 8)

# ================== CHANGE THESE TWO LINES ONLY ==================
layer_id = 7                                      # ← change to 1, 2, 5, etc.
input_file  = r'E:\gis-map-export\PE_sites.json'    # ← your saved JSON file
output_file = r'E:\gis-map-export\PE_sites_decimal.csv'
# =================================================================

with open(input_file, 'r', encoding='utf-8') as f:
    data = json.load(f)

features = data.get('features', [])
print(f"Loaded {len(features)} features from Layer {layer_id}")

with open(output_file, 'w', newline='', encoding='utf-8') as csvfile:
    writer = csv.writer(csvfile)
    # Write header (all fields + decimal coordinates)
    header = ['LONGITUDE_DECIMAL', 'LATITUDE_DECIMAL'] + list(features[0]['attributes'].keys()) if features else []
    writer.writerow(header)
    
    for feat in features:
        attr = feat.get('attributes', {})
        lon = dms_to_decimal(attr.get('LONGITUDE'))
        lat = dms_to_decimal(attr.get('LATITUDE'))
        row = [lon, lat] + [attr.get(k) for k in header[2:]]
        writer.writerow(row)

print(f"Export finished → {output_file}")