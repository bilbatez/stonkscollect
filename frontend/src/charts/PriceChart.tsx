import { useEffect, useRef } from 'react'
import * as echarts from 'echarts'
import type { PricePoint } from '../types'

/** Daily price chart: candlestick when OHLC is present, else a close line.
 *  Canvas rendering glue — excluded from coverage. */
export default function PriceChart({ prices }: { prices: PricePoint[] }) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!ref.current) {
      return
    }
    const chart = echarts.init(ref.current)
    const dates = prices.map((p) => p.date)
    const hasOhlc = prices.some(
      (p) => p.open != null && p.high != null && p.low != null,
    )

    const series = hasOhlc
      ? {
          type: 'candlestick' as const,
          // echarts candlestick: [open, close, low, high]
          data: prices.map((p) => [p.open ?? p.close, p.close, p.low ?? p.close, p.high ?? p.close]),
        }
      : {
          name: 'Close',
          type: 'line' as const,
          showSymbol: false,
          data: prices.map((p) => p.close),
        }

    chart.setOption({
      tooltip: { trigger: 'axis' },
      xAxis: { type: 'category', data: dates },
      yAxis: { type: 'value', scale: true },
      series: [series],
    })
    return () => chart.dispose()
  }, [prices])

  return <div role="img" aria-label="price chart" ref={ref} style={{ height: 320 }} />
}
