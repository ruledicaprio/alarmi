#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""
Genset Inventory Consolidation
Base: NE_elements_corrected_2d.csv (location & spatial master)
Attach: dbo_agregat (existing), Potencijalne lokacije (planned)
"""
import pandas as pd
import numpy as np
import re
from fuzzywuzzy import fuzz, process
import warnings
import os

warnings.filterwarnings('ignore')

# ================= CONFIGURATION =================
DIR = os.path.dirname(os.path.abspath(__file__))
FILE_NE = os.path.join(DIR, 'NE_elements_corrected_2d.csv')
FILE_AGG = os.path.join(DIR, 'dbo_agregat (4).xlsx')
FILE_PLAN = os.path.join(DIR, 'Potencijalne lokacije za agregati i baterije BSe_RR cvorista_24062025_16_03_2026.xlsx')
OUT_CSV = os.path.join(DIR, 'Genset_Inventory_Verified.csv')
OUT_XLSX = os.path.join(DIR, 'Genset_Inventory_Verified.xlsx')

# ================= HELPERS =================
def norm(name):
    """Strict normalization for spatial/name matching"""
    if pd.isna(name): return ""
    s = str(name).upper().strip()
    s = re.sub(r'[^\w\s\-]', '', s)
    s = re.sub(r'\s+', ' ', s)
    return s.strip()

def match_to_ne(loc, ne_df, threshold=75):
    """Return best NE match + confidence"""
    n_loc = norm(loc)
    if not n_loc: return pd.Series(dtype=float), 0, "EMPTY"
    
    # Exact match on normalized NE_NAME or LOKACIJA
    exact = ne_df[(ne_df['NE_NAME_NORM'] == n_loc) | (ne_df['LOKACIJA_NORM'] == n_loc)]
    if not exact.empty:
        return exact.iloc[0], 100, "EXACT"
        
    # Fuzzy match
    choices = ne_df['NE_NAME_NORM'].tolist() + ne_df['LOKACIJA_NORM'].tolist()
    best = process.extractOne(n_loc, choices, scorer=fuzz.token_sort_ratio)
    if best and best[1] >= threshold:
        idx = ne_df.index[(ne_df['NE_NAME_NORM'] == best[0]) | (ne_df['LOKACIJA_NORM'] == best[0])]
        if len(idx) > 0:
            return ne_df.loc[idx[0]], best[1], "FUZZY"
            
    return pd.Series(dtype=float), 0, "NO_MATCH"

def safe_join(df, suffix):
    """Flattens matched columns with prefix"""
    if df.empty or df.isna().all(): return {}
    res = {}
    for col in ['NE_NAME', 'LOKACIJA', 'LONGITUDE_DECIMAL', 'LATITUDE_DECIMAL', 'TEHNOLOGIJA', 'STATUS']:
        if col in df.index:
            res[f"NE_{col}_{suffix}"] = df[col]
    return res

# ================= MAIN =================
def main():
    print("📡 Loading NE master & inventory files...")
    ne = pd.read_csv(FILE_NE, encoding='utf-8-sig')
    agg = pd.read_excel(FILE_AGG)
    plan = pd.read_excel(FILE_PLAN, sheet_name=0)

    # Prepare NE base
    ne['NE_NAME_NORM'] = ne['NE_NAME'].apply(norm)
    ne['LOKACIJA_NORM'] = ne['LOKACIJA'].apply(norm)
    ne_base = ne[['NE_NAME','LOKACIJA','NE_NAME_NORM','LOKACIJA_NORM',
                  'LONGITUDE_DECIMAL','LATITUDE_DECIMAL','TEHNOLOGIJA','STATUS','X','Y']].drop_duplicates(subset=['NE_NAME_NORM','LOKACIJA_NORM'])

    # 1️⃣ Match existing gensets (dbo_agregat) to NE base
    print("🔌 Matching existing gensets (dbo_agregat)...")
    existing_rows = []
    for _, r in agg.iterrows():
        loc = f"{r.get('Mjesto','')} {r.get('Naziv Objekta','')}".strip()
        match, conf, method = match_to_ne(loc, ne_base)
        row = {
            'GENSET_ID': r.get('ID'),
            'DIREKCIJA': r.get('Nadležnost'),
            'NAZIV_OBJEKTA': r.get('Naziv Objekta'),
            'SNAGA_KVA': r.get('Snaga (kVA)'),
            'PROIZVODJAC': r.get('Proizvođač'),
            'TIP_AGREGATA': r.get('Tip'),
            'UPRAVLJACKA': r.get('Upravljačka Jedinica'),
            'NADZOR': r.get('Nadzor'),
            'IP_ADRESA': r.get('IP adresa'),
            'IZVEDBA': r.get('Izvedba'),
            'KAP_SPREMNIKA_L': r.get('Kapacitet spremnika (l)'),
            'SERIJSKI_BROJ': r.get('Serijski Broj'),
            'GODINA_PROIZV': r.get('Godina Proizvodnje'),
            'STATUS_INVENTAR': 'EXISTING'
        }
        row.update(safe_join(match, 'AGG'))
        row.update({'MATCH_CONF': conf, 'MATCH_METHOD': method})
        existing_rows.append(row)

    df_existing = pd.DataFrame(existing_rows)

    # 2️⃣ Match planned sites (Potencijalne lokacije) to NE base
    print("📐 Matching planned sites...")
    planned_rows = []
    for _, r in plan.iterrows():
        loc = r.get('Naziv lokacije', '')
        if pd.isna(loc): continue
        match, conf, method = match_to_ne(loc, ne_base)
        row = {
            'PLAN_NAZIV': loc,
            'DIREKCIJA_PLAN': r.get('Direkcija'),
            'ENTITET': r.get('Entitet'),
            'OPCINA': r.get('Opcina'),
            'PRIORITET': r.get('Napomena Prioritet'),
            'PROSTOR_MONTAZA': r.get('Ima li fizički prostor za montažu agregata'),
            'DOZVOLA': r.get('Treba li građ. dozvola'),
            'STATUS_REALIZACIJE': r.get('STATUS REALIZACIJE'),
            'STATUS_INVENTAR': 'PLANNED'
        }
        row.update(safe_join(match, 'PLAN'))
        row.update({'MATCH_CONF': conf, 'MATCH_METHOD': method})
        planned_rows.append(row)

    df_planned = pd.DataFrame(planned_rows)

    # 3️⃣ Merge & deduplicate
    print("🧩 Merging & deduplicating...")
    df_all = pd.concat([df_existing, df_planned], ignore_index=True)
    
    # Spatial validation flag
    df_all['COORDS_VALID'] = (
        df_all['NE_LONGITUDE_DECIMAL_PLAN'].notna() & 
        df_all['NE_LATITUDE_DECIMAL_PLAN'].notna()
    )
    
    # Priority sort: Existing first, then high-priority planned
    df_all = df_all.sort_values(['STATUS_INVENTAR', 'PRIORITET', 'MATCH_CONF'], ascending=[True, True, False]).reset_index(drop=True)

    # 4️⃣ Export
    print(f"📦 Exporting {len(df_all)} verified records...")
    df_all.to_csv(OUT_CSV, index=False, encoding='utf-8-sig')
    df_all.to_excel(OUT_XLSX, index=False, engine='openpyxl')
    
    # Summary
    print("\n✅ VERIFICATION SUMMARY")
    print(f"📍 NE Master Locations: {len(ne_base)}")
    print(f"⚡ Existing Gensets Matched: {len(df_existing[df_existing['MATCH_CONF'] >= 75])}")
    print(f"📅 Planned Sites Matched: {len(df_planned[df_planned['MATCH_CONF'] >= 75])}")
    print(f"🎯 Total Inventory Rows: {len(df_all)}")
    print(f"🗺️ Valid Coordinates: {df_all['COORDS_VALID'].sum()}/{len(df_all)}")
    print(f"💾 Saved: {OUT_CSV} & {OUT_XLSX}")

if __name__ == '__main__':
    main()