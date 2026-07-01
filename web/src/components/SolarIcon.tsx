// Solar PV panel icon — paths extracted from solar-energy-panel-9400.svg.
// Inlined as SVG paths; no external file or dynamic import.
// Coordinate space: 0–90 (original SVG viewBox after removing scale transform).
export default function SolarIcon() {
  return (
    <svg
      viewBox="0 0 90 90"
      width="1em"
      height="1em"
      aria-hidden
      style={{ display: 'inline-block', verticalAlign: '-0.125em' }}
    >
      {/* Sun semicircle */}
      <path
        d="M59.095 27.918c0-7.784-6.311-14.095-14.095-14.095S30.905 20.134 30.905 27.918z"
        fill="#FDB62F"
      />
      {/* Sun rays */}
      <line x1="45" y1="9"  x2="45" y2="1"   stroke="#FDB62F" strokeWidth="2" strokeLinecap="round" />
      <line x1="25" y1="29" x2="18" y2="29"   stroke="#FDB62F" strokeWidth="2" strokeLinecap="round" />
      <line x1="65" y1="29" x2="72" y2="29"   stroke="#FDB62F" strokeWidth="2" strokeLinecap="round" />
      <line x1="31" y1="14" x2="26" y2="9"    stroke="#FDB62F" strokeWidth="2" strokeLinecap="round" />
      <line x1="59" y1="14" x2="64" y2="9"    stroke="#FDB62F" strokeWidth="2" strokeLinecap="round" />
      {/* Solar panel */}
      <polygon points="85.45,68.17 4.55,68.17 17.33,36.06 72.67,36.06" fill="#4398D1" />
      {/* Panel grid lines */}
      <line x1="45"    y1="36.06" x2="45"    y2="68.17" stroke="#fff" strokeWidth="1" strokeOpacity="0.4" />
      <line x1="17.33" y1="52.1"  x2="72.67" y2="52.1"  stroke="#fff" strokeWidth="1" strokeOpacity="0.4" />
      {/* Base bar */}
      <rect x="4.55"  y="68.17" width="80.91" height="6.95" fill="#D1D1D1" />
      {/* Stand post */}
      <rect x="38.55" y="75.11" width="12.89" height="6.95" fill="#B2B2B2" />
      {/* Foot */}
      <path
        d="M57.838 89H32.162v-4.129c0-1.555 1.261-2.816 2.816-2.816h20.045c1.555 0 2.816 1.261 2.816 2.816V89z"
        fill="#D1D1D1"
      />
    </svg>
  )
}
