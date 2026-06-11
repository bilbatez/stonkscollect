import type { EChartsType } from 'echarts/core'

/**
 * Keep `chart` sized to its container by resizing it on every window resize.
 * Returns a cleanup that removes the listener and disposes the chart — use it
 * directly as a `useEffect` return value:
 *
 * ```ts
 * useEffect(() => {
 *   const chart = echarts.init(ref.current)
 *   // …setOption…
 *   return bindChartResize(chart)
 * }, [deps])
 * ```
 *
 * Canvas glue, excluded from coverage like the chart wrappers themselves.
 */
export function bindChartResize(chart: EChartsType): () => void {
  const onResize = () => chart.resize()
  window.addEventListener('resize', onResize)
  return () => {
    window.removeEventListener('resize', onResize)
    chart.dispose()
  }
}
