import csv
import simplekml
import os

# ================== CONFIGURE HERE ==================
csv_file = r'E:\gis-map-export\PE_sites_decimal.csv'   # Change if your filename is different
kmz_output = r'E:\gis-map-export\BH_Line_Sites.kmz'

# Fields to show in the pop-up on the phone
fields_to_include = [
    'MJESTO', 'TIP_KOMUTA', 'TEHNOLOGIJA' ]

#=================================================

kml = simplekml.Kml(name="BH Telecom Network Elements")

count = 0
with open(csv_file, 'r', encoding='utf-8') as f:
    reader = csv.DictReader(f)
    for row in reader:
        try:
            # Safe conversion - skip if coordinates are missing or invalid
            lon_str = row.get('LONGITUDE_DECIMAL')
            lat_str = row.get('LATITUDE_DECIMAL')
            
            if not lon_str or not lat_str:
                continue
                
            lon = float(lon_str)
            lat = float(lat_str)
            
            # Create point
            name = row.get('LOKACIJA') or row.get('NE_NAME') or "Unknown Site"
            pnt = kml.newpoint(name=name, coords=[(lon, lat)])
            
            # Build rich HTML description
            description = "<h3>Network Element Details</h3>"
            description += "<table border='1' cellpadding='4' style='border-collapse:collapse; width:100%;'>"
            for field in fields_to_include:
                value = row.get(field)
                if value and str(value).strip() != '':
                    description += f"<tr><td><b>{field}</b></td><td>{value}</td></tr>"
            description += "</table>"
            
            pnt.description = description
            
            # Nice icon for mobile visibility
            pnt.style.iconstyle.icon.href = "http://maps.google.com/mapfiles/kml/shapes/placemark_circle.png"
            pnt.style.iconstyle.scale = 1.2
            
            count += 1
            
        except (ValueError, TypeError, AttributeError):
            continue  # Skip problematic rows silently

# Save as KMZ (compressed KML)
kml.savekmz(kmz_output)

print(f"Success! KMZ file created with {count} network elements.")
print(f"File location: {kmz_output}")
print("\nYou can now send this .kmz file to your colleagues.")
print("They can open it directly in the Google Earth mobile app.")