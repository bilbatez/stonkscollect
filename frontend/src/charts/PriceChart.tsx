import { useEffect, useRef } from 'react'
import * as echarts from 'echarts'
import { PRICE_CHART_HEIGHT } from '../constants'
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

    const zoomStart = prices.length > 90
      ? Math.round((1 - 90 / prices.length) * 100)
      : 0

    chart.setOption({
      tooltip: { trigger: 'axis' },
      grid: { bottom: 60 },
      xAxis: { type: 'category', data: dates },
      yAxis: { type: 'value', scale: true },
      dataZoom: [
        { type: 'inside', xAxisIndex: 0, start: zoomStart, end: 100 },
        { type: 'slider', xAxisIndex: 0, start: zoomStart, end: 100, height: 20, bottom: 8 },
      ],
      series: [series],
    })

    const onResize = () => chart.resize()
    window.addEventListener('resize', onResize)
    return () => {
      window.removeEventListener('resize', onResize)
      chart.dispose()
    }
  }, [prices])

  return <div role="img" aria-label="price chart" ref={ref} style={{ height: PRICE_CHART_HEIGHT }} />
}
