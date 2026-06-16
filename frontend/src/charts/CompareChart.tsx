import { useEffect, useRef } from 'react'
import * as echarts from 'echarts'
import { bindChartResize } from './chartResize'
import { normalizedReturns } from '../chartData'
import { CHART_HEIGHT } from '../constants'
import type { PricePoint } from '../types'

/** Yahoo-style "compare" overlay: each ticker's price rebased to % change from
 *  the first date they all have data. Canvas glue — excluded from coverage. */
export default function CompareChart({
  seriesByTicker,
}: {
  seriesByTicker: Record<string, PricePoint[]>
}) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!ref.current) {
      return
    }
    const chart = echarts.init(ref.current)
    const { categories, series } = normalizedReturns(seriesByTicker)
    chart.setOption({
      tooltip: { trigger: 'axis', valueFormatter: (v: number | null) => (v == null ? '—' : `${v.toFixed(2)}%`) },
      legend: { top: 0 },
      grid: { left: 50, right: 16, top: 30, bottom: 30 },
      xAxis: { type: 'category', data: categories },
      yAxis: { type: 'value', scale: true, axisLabel: { formatter: '{value}%' } },
      series: series.map((s) => ({ name: s.name, type: 'line', showSymbol: false, connectNulls: true, data: s.data })),
    })
    return bindChartResize(chart)
  }, [seriesByTicker])

  return <div role="img" aria-label="compare chart" ref={ref} style={{ height: CHART_HEIGHT }} />
}
