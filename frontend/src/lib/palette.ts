// Categorical palette from the data-viz reference instance, validated for
// adjacent-pair colour-vision separation in this fixed order, with the dark
// steps chosen for the dark surface. Slots are assigned by customer id order
// and never cycled: a ninth customer folds into "Other".
export const SERIES = [
  '#2a78d6',
  '#eb6834',
  '#1baf7a',
  '#eda100',
  '#e87ba4',
  '#008300',
  '#4a3aa7',
  '#e34948',
]
export const SERIES_DARK = [
  '#3987e5',
  '#d95926',
  '#199e70',
  '#c98500',
  '#d55181',
  '#008300',
  '#9085e9',
  '#e66767',
]
export const OTHER = '#8a8983'

export function seriesColor(index: number, theme: 'light' | 'dark' = 'light'): string {
  const table = theme === 'dark' ? SERIES_DARK : SERIES
  return table[index] ?? OTHER
}

/** Text and grid colours for charts, matching the CSS theme tokens. */
export function chartInk(theme: 'light' | 'dark') {
  return theme === 'dark'
    ? { text: '#ecebe6', muted: '#b3b1a8', grid: '#33332f', surface: '#1c1c1a' }
    : { text: '#1a1917', muted: '#5d5b55', grid: '#e4e2dc', surface: '#ffffff' }
}
