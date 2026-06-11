import { useEffect, useRef } from 'react'
import * as echarts from 'echarts/core'
import { LineChart } from 'echarts/charts'
import { GridComponent, TooltipComponent, LegendComponent, MarkAreaComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { bindChartResize } from './chartResize'
import { grahamChartData } from '../chartData'
import { CHART_HEIGHT } from '../constants'
import { formatCurrency } from '../format'
import type { FinancialFact, PricePoint, Ratio } from '../types'

echarts.use([LineChart, GridComponent, TooltipComponent, LegendComponent, MarkAreaComponent, CanvasRenderer])

export default function GrahamChart({
  prices,
  facts,
  ratios,
}: {
  prices: PricePoint[]
  facts: FinancialFact[]
  ratios: Ratio[]
}) {
  const ref = useRef<HTMLDivElement>(null)
  const d = grahamChartData(prices, facts, ratios)

  useEffect(() => {
    if (!ref.current || !d) return
    const chart = echarts.init(ref.current)
    chart.setOption({
      tooltip: {
        trigger: 'axis',
        valueFormatter: (v: number | null) => (v === null ? '—' : formatCurrency(v)),
      },
      legend: { bottom: 0 },
      grid: { bottom: 60 },
      xAxis: { type: 'category', data: d.dates },
      yAxis: { type: 'value', scale: true },
      series: [
        {
          type: 'line',
          name: 'Close price',
          data: d.prices,
          symbol: 'none',
          lineStyle: { width: 2 },
        },
        {
          type: 'line',
          name: 'Graham Number',
          data: d.grahamNumbers,
          symbol: 'none',
          lineStyle: { type: 'dashed', width: 2 },
          areaStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: 'rgba(34,197,94,0.25)' },
              { offset: 1, color: 'rgba(34,197,94,0.0)' },
            ]),
          },
        },
      ],
    })
    return bindChartResize(chart)
  }, [d])

  if (!d) return null
  return <div ref={ref} style={{ height: CHART_HEIGHT }} />
}
