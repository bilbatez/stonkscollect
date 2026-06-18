import { lazy, Suspense, useState } from 'react'
import {
  Box,
  Card,
  CardContent,
  Chip,
  Link,
  Stack,
  Tab,
  Tabs,
  Typography,
} from '@mui/material'
import { freshness, secFilingsUrl, wikipediaUrl, yahooProfileUrl } from '../../format'
import { computeKeyStats, computePeriodReturns, computeQuote } from '../../quote'
import { DiscrepancyPanel } from '../panels/DiscrepancyPanel'
import { DividendPanel } from '../panels/DividendPanel'
import { FreshnessBadge } from '../shared/FreshnessBadge'
import { GrahamScorecard } from '../panels/GrahamScorecard'
import { HoldersPanel } from '../panels/HoldersPanel'
import { KeyStatsPanel } from '../panels/KeyStatsPanel'
import { MetricsSummary } from '../panels/MetricsSummary'
import { QuoteHeader } from '../panels/QuoteHeader'
import { NewsFeed } from '../panels/NewsFeed'
import { NotePanel } from '../panels/NotePanel'
import { PeersPanel } from '../panels/PeersPanel'
import { PeriodToggle } from '../shared/PeriodToggle'
import { RangeToggle } from '../shared/RangeToggle'
import { RatiosPanel } from '../panels/RatiosPanel'
import { Skeleton } from '../shared/Skeleton'
import { WeekRangeBar } from '../shared/WeekRangeBar'
import { StatementTable } from '../panels/StatementTable'
import { pricesForRange, type RangePreset } from '../../chartData'
import type { CompanyData, Period } from '../../types'

const PriceChart = lazy(() => import('../../charts/PriceChart'))
const IncomeChart = lazy(() => import('../../charts/IncomeChart'))
const RatioChart = lazy(() => import('../../charts/RatioChart'))
const GrahamChart = lazy(() => import('../../charts/GrahamChart'))

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Box sx={{ mt: 2 }}>
      <Typography variant="subtitle1" component="h3" gutterBottom sx={{ fontWeight: 600 }}>
        {title}
      </Typography>
      {children}
    </Box>
  )
}

/** Lay children side-by-side on wider screens, stacked on small ones. */
function TwoUp({ children }: { children: React.ReactNode }) {
  return (
    <Box
      sx={{
        display: 'grid',
        gap: 2,
        gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' },
        alignItems: 'start',
      }}
    >
      {children}
    </Box>
  )
}

const TABS = ['Overview', 'Financials', 'Valuation & quality', 'Ownership & news'] as const

function a11yProps(index: number) {
  return { id: `company-tab-${index}`, 'aria-controls': `company-tabpanel-${index}` }
}

function TabPanel({
  index,
  active,
  children,
}: {
  index: number
  active: number
  children: React.ReactNode
}) {
  if (active !== index) return null
  return (
    <Box role="tabpanel" id={`company-tabpanel-${index}`} aria-labelledby={`company-tab-${index}`}>
      {children}
    </Box>
  )
}

export function CompanyView({ data, loadedAt }: { data: CompanyData; loadedAt: number }) {
  const [activeTab, setActiveTab] = useState(0)
  const [chartPeriod, setChartPeriod] = useState<Period>('annual')
  const [priceRange, setPriceRange] = useState<RangePreset>('1Y')
  // prices arrive oldest-first from the API (ORDER BY date ASC); last element is newest
  const latestPriceDate = data.prices.at(-1)?.date ?? null
  const c = data.company
  const quote = computeQuote(data.prices)
  const periodReturns = computePeriodReturns(data.prices)
  const keyStats = computeKeyStats(data, quote)
  const rangedPrices = pricesForRange(data.prices, priceRange)
  return (
    <Card variant="outlined" component="article">
      <CardContent>
        <Stack direction="row" spacing={2} sx={{ alignItems: 'center', flexWrap: 'wrap' }}>
          <Typography variant="h5" component="h2">
            {c.name} ({c.ticker})
          </Typography>
          <FreshnessBadge status={freshness(latestPriceDate, loadedAt)} />
        </Stack>

        <Stack direction="row" spacing={1} sx={{ mt: 1, flexWrap: 'wrap' }}>
          {c.sector && <Chip size="small" label={c.sector} />}
          {c.industry && <Chip size="small" variant="outlined" label={c.industry} />}
          {c.exchange && <Chip size="small" variant="outlined" label={c.exchange} />}
        </Stack>
        {c.description && (
          <Typography variant="body2" color="text.secondary" sx={{ mt: 1.5 }}>
            {c.description}
          </Typography>
        )}
        <Stack direction="row" spacing={2} sx={{ mt: 1.5, flexWrap: 'wrap' }}>
          <Link href={secFilingsUrl(c.cik)} target="_blank" rel="noreferrer">
            SEC filings
          </Link>
          {c.website && (
            <Link href={c.website} target="_blank" rel="noreferrer">
              Website
            </Link>
          )}
          <Link href={wikipediaUrl(c.name)} target="_blank" rel="noreferrer">
            Wikipedia
          </Link>
          <Link href={yahooProfileUrl(c.ticker)} target="_blank" rel="noreferrer">
            Yahoo Finance
          </Link>
        </Stack>

        <Tabs
          value={activeTab}
          onChange={(_, v) => setActiveTab(v)}
          aria-label="company sections"
          variant="scrollable"
          scrollButtons="auto"
          sx={{ mt: 2, borderBottom: 1, borderColor: 'divider' }}
        >
          {TABS.map((label, i) => (
            <Tab key={label} label={label} {...a11yProps(i)} />
          ))}
        </Tabs>

        <TabPanel index={0} active={activeTab}>
          <QuoteHeader quote={quote} returns={periodReturns} />
          <TwoUp>
            <MetricsSummary ratios={data.ratios} graham={data.graham} />
            <Section title="Key statistics">
              {quote && (
                <Box sx={{ mb: 1.5 }}>
                  <WeekRangeBar low={quote.week52Low} high={quote.week52High} last={quote.last} />
                </Box>
              )}
              <KeyStatsPanel stats={keyStats} quote={quote} />
            </Section>
          </TwoUp>
          <Section title="Price">
            <RangeToggle value={priceRange} onChange={setPriceRange} />
            <Suspense fallback={<Skeleton label="Loading chart…" />}>
              <PriceChart prices={rangedPrices} />
            </Suspense>
            <Suspense fallback={null}>
              <GrahamChart prices={rangedPrices} facts={data.facts} ratios={data.ratios} />
            </Suspense>
          </Section>
        </TabPanel>

        <TabPanel index={1} active={activeTab}>
          <Section title="Income">
            <PeriodToggle period={chartPeriod} onChange={setChartPeriod} />
            <Suspense fallback={null}>
              <IncomeChart facts={data.facts} period={chartPeriod} />
            </Suspense>
          </Section>
          <Section title="Statements">
            <StatementTable facts={data.facts} />
          </Section>
          <Section title="Dividends">
            <DividendPanel facts={data.facts} />
          </Section>
        </TabPanel>

        <TabPanel index={2} active={activeTab}>
          <Box sx={{ mt: 2 }}>
            <GrahamScorecard assessment={data.graham} />
          </Box>
          <Section title="Ratios">
            <Suspense fallback={null}>
              <RatioChart ratios={data.ratios} period={chartPeriod} />
            </Suspense>
            <RatiosPanel ratios={data.ratios} />
          </Section>
        </TabPanel>

        <TabPanel index={3} active={activeTab}>
          <TwoUp>
            <Section title="Peers">
              <PeersPanel peers={data.peers} />
            </Section>
            <Section title="Holders">
              <HoldersPanel ticker={c.ticker} />
            </Section>
          </TwoUp>
          <Section title="News">
            <NewsFeed news={data.news} />
          </Section>
          <Section title="Notes">
            <NotePanel ticker={c.ticker} initialBody={data.note.body} />
          </Section>
          <Section title="Discrepancies">
            <DiscrepancyPanel discrepancies={data.discrepancies} />
          </Section>
        </TabPanel>
      </CardContent>
    </Card>
  )
}
