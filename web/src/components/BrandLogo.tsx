import { useTheme } from '../features/theme/ThemeProvider';

type BrandLogoProps = {
  size: number;
  className?: string;
};

export function BrandLogo({ size, className }: BrandLogoProps) {
  const { resolved } = useTheme();
  const base = resolved === 'dark' ? '/logo-dark' : '/logo';

  return (
    <img
      src={`${base}.webp`}
      srcSet={`${base}.webp 1x, ${base}@2x.webp 2x`}
      alt="Coppice"
      width={size}
      height={size}
      className={className}
    />
  );
}
