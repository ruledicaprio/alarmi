// BHTelecom-style logo: lowercase "bh" + 3-dot decoration in BH brand orange.
// Inline SVG so no asset shipping needed.

export default function BhLogo({ size = 26 }: { size?: number }) {
  const orange = '#f57e20'
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" xmlns="http://www.w3.org/2000/svg"
         style={{ flexShrink: 0 }}>
      {/* "b" */}
      <path d="M 8 12 L 8 50 Q 8 56 14 56 L 22 56 Q 32 56 32 44 Q 32 32 22 32 L 16 32"
            stroke={orange} strokeWidth="6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
      {/* "h" */}
      <path d="M 34 12 L 34 56 M 34 36 Q 34 32 38 32 L 44 32 Q 48 32 48 36 L 48 56"
            stroke={orange} strokeWidth="6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
      {/* 3 dots */}
      <circle cx="56" cy="20" r="3" fill={orange} />
      <circle cx="56" cy="32" r="3" fill={orange} />
      <circle cx="56" cy="44" r="3" fill={orange} />
    </svg>
  )
}
