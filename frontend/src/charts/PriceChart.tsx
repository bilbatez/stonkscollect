import { useEffect, useRef } from 'react'
import * as echarts from 'echarts'
import type { PricePoint } from '../types'

/** Daily close line chart. Canvas rendering glue — excluded from coverage. */
export default function PriceChart({ prices }: { prices: PricePoint[] }) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!ref.current) {
      return
    }
    const chart = echarts.init(ref.current)
    chart.setOption({
      tooltip: { trigger: 'axis' },
      xAxis: { type: 'category', data: prices.map((p) => p.date) },
      yAxis: { type: 'value', scale: true },
      series: [
        {
          name: 'Close',
          type: 'line',
          showSymbol: false,
          data: prices.map((p) => p.close),
        },
      ],
    })
    return () => chart.dispose()
  }, [prices])

  return <div role="img" aria-label="price chart" ref={ref} style={{ height: 320 }} />
}
