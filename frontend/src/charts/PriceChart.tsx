import { useEffect, useRef, useState } from 'react'
import ToggleButton from '@mui/material/ToggleButton'
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup'
import Box from '@mui/material/Box'
import * as echarts from 'echarts'
import { bindChartResize } from './chartResize'
import { movingAverage } from '../chartData'
import { PRICE_CHART_HEIGHT } from '../constants'
import type { PricePoint } from '../types'

const UP = '#22c55e'
const DOWN = '#ef4444'

/** Daily price chart: candlestick when OHLC is present, else a close line, with
 *  a volume sub-chart and optional SMA 50/200 overlays (Yahoo-style).
 *  Canvas rendering glue — excluded from coverage. */
export default function PriceChart({ prices }: { prices: PricePoint[] }) {
  const ref = useRef<HTMLDivElement>(null)
  const [showSma, setShowSma] = useState(false)

  useEffect(() => {
    if (!ref.current) {
      return
    }
    const chart = echarts.init(ref.current)
    const dates = prices.map((p) => p.date)
    const hasOhlc = prices.some(
      (p) => p.open != null && p.high != null && p.low != null,
    )

    const priceSeries = hasOhlc
      ? {
          name: 'Price',
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

    const smaSeries = showSma
      ? [50, 200].map((window) => ({
          name: `SMA ${window}`,
          type: 'line' as const,
          showSymbol: false,
          smooth: true,
          lineStyle: { width: 1 },
          data: movingAverage(prices, window),
        }))
      : []

    // Volume bars sit in their own grid below the price grid, coloured by the
    // up/down day (close vs prior close).
    const volume = prices.map((p, i) => {
      const prev = i > 0 ? prices[i - 1].close : p.close
      return { value: p.volume ?? 0, itemStyle: { color: p.close >= prev ? UP : DOWN } }
    })

    // The visible window is chosen by the range presets upstream; the zoom
    // tools always start showing everything they were given.
    chart.setOption({
      tooltip: { trigger: 'axis' },
      legend: { top: 0 },
      axisPointer: { link: [{ xAxisIndex: 'all' }] },
      grid: [
        { left: 50, right: 16, top: 30, height: '60%' },
        { left: 50, right: 16, top: '75%', height: '12%' },
      ],
      xAxis: [
        { type: 'category', data: dates, gridIndex: 0, boundaryGap: true },
        { type: 'category', data: dates, gridIndex: 1, axisLabel: { show: false } },
      ],
      yAxis: [
        { type: 'value', scale: true, gridIndex: 0 },
        { type: 'value', scale: true, gridIndex: 1, axisLabel: { show: false }, splitLine: { show: false } },
      ],
      dataZoom: [
        { type: 'inside', xAxisIndex: [0, 1], start: 0, end: 100 },
        { type: 'slider', xAxisIndex: [0, 1], start: 0, end: 100, height: 18, bottom: 4 },
      ],
      series: [
        priceSeries,
        ...smaSeries,
        { name: 'Volume', type: 'bar', xAxisIndex: 1, yAxisIndex: 1, data: volume },
      ],
    })

    return bindChartResize(chart)
  }, [prices, showSma])

  return (
    <Box>
      <ToggleButtonGroup
        size="small"
        value={showSma ? ['sma'] : []}
        onChange={(_, v) => setShowSma(v.includes('sma'))}
        sx={{ mb: 1 }}
      >
        <ToggleButton value="sma">SMA 50 / 200</ToggleButton>
      </ToggleButtonGroup>
      <div role="img" aria-label="price chart" ref={ref} style={{ height: PRICE_CHART_HEIGHT }} />
    </Box>
  )
}
