import logo from '../assets/bht-logo.svg'

// Official BH Telecom logo — SVG asset inlined by Vite at build time.
// The logo is 206×100 (≈2.06:1), so height drives the rendered size.
export default function BhLogo({ size = 26 }: { size?: number }) {
  return (
    <img
      src={logo}
      height={size}
      width={Math.round(size * 2.06)}
      style={{ flexShrink: 0, display: 'block' }}
      alt="BH Telecom"
    />
  )
}
