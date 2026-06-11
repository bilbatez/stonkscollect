# Financial Data

## Collected line items

StonksCollect collects 35 normalized line items from SEC EDGAR XBRL data, mapped from standard us-gaap concepts.

### Income statement

| Normalized key | XBRL concept(s) | Label |
|---|---|---|
| `Revenue` | `Revenues`, `RevenueFromContractWithCustomerExcludingAssessedTax` | Revenue |
| `GrossProfit` | `GrossProfit` | Gross profit |
| `OperatingIncome` | `OperatingIncomeLoss` | Operating income |
| `NetIncome` | `NetIncomeLoss` | Net income |
| `Eps` | `EarningsPerShareDiluted`, `EarningsPerShareBasic` | EPS (diluted) |
| `DividendPerShare` | `CommonStockDividendsPerShareDeclared` | Dividend / share |
| `SharesOutstanding` | `WeightedAverageNumberOfDilutedSharesOutstanding` | Shares outstanding (wtd avg) |
| `DepreciationAmortization` | `DepreciationDepletionAndAmortization` | Depreciation & amortization |
| `ResearchAndDevelopment` | `ResearchAndDevelopmentExpense` | R&D expense |
| `SellingGeneralAdmin` | `SellingGeneralAndAdministrativeExpense` | SG&A expense |
| `InterestExpense` | `InterestExpense`, `InterestAndDebtExpense` | Interest expense |
| `IncomeTaxExpense` | `IncomeTaxExpenseBenefit` | Income tax expense |

### Balance sheet

| Normalized key | XBRL concept(s) | Label |
|---|---|---|
| `TotalAssets` | `Assets` | Total assets |
| `TotalLiabilities` | `Liabilities` | Total liabilities |
| `StockholdersEquity` | `StockholdersEquity` | Shareholders' equity |
| `CurrentAssets` | `AssetsCurrent` | Current assets |
| `CurrentLiabilities` | `LiabilitiesCurrent` | Current liabilities |
| `LongTermDebt` | `LongTermDebtNoncurrent`, `LongTermDebt` | Long-term debt |
| `CashAndEquivalents` | `CashAndCashEquivalentsAtCarryingValue` | Cash & equivalents |
| `Goodwill` | `Goodwill` | Goodwill |
| `IntangibleAssets` | `IntangibleAssetsNetExcludingGoodwill` | Intangible assets |
| `PropertyPlantEquipment` | `PropertyPlantAndEquipmentNet` | PP&E (net) |
| `Inventories` | `InventoryNet` | Inventories |
| `AccountsReceivable` | `AccountsReceivableNetCurrent` | Accounts receivable |
| `AccountsPayable` | `AccountsPayableCurrent` | Accounts payable |
| `ShortTermDebt` | `ShortTermBorrowings`, `LongTermDebtCurrent` | Short-term debt |
| `RetainedEarnings` | `RetainedEarningsAccumulatedDeficit` | Retained earnings |
| `SharesOutstandingBalance` | `CommonStockSharesOutstanding` | Shares outstanding (balance) |

### Cash flow statement

| Normalized key | XBRL concept(s) | Label |
|---|---|---|
| `OperatingCashFlow` | `NetCashProvidedByUsedInOperatingActivities` | Operating cash flow |
| `InvestingCashFlow` | `NetCashProvidedByUsedInInvestingActivities` | Investing cash flow |
| `FinancingCashFlow` | `NetCashProvidedByUsedInFinancingActivities` | Financing cash flow |
| `CapEx` | `PaymentsToAcquirePropertyPlantAndEquipment` | Capital expenditure |
| `DividendsPaid` | `PaymentsOfDividendsCommonStock` | Dividends paid |

---

## Computed ratios

Ratios are computed per period (annual and quarterly) by `ratios::compute()` from stored facts. P/E and P/B additionally require the closing price on or before the period end date.

| Metric key | Formula | Notes |
|---|---|---|
| `pe` | `price / EPS` | Only when EPS > 0 |
| `pb` | `price / BVPS` | Only when BVPS > 0 |
| `roe` | `NetIncome / StockholdersEquity` | Return on equity |
| `net_margin` | `NetIncome / Revenue` | Net profit margin |
| `gross_margin` | `GrossProfit / Revenue` | |
| `operating_margin` | `OperatingIncome / Revenue` | |
| `debt_to_equity` | `TotalLiabilities / StockholdersEquity` | |
| `current_ratio` | `CurrentAssets / CurrentLiabilities` | |
| `book_value_per_share` | `StockholdersEquity / SharesOutstanding` | BVPS |
| `payout_ratio` | `DividendPerShare / EPS` | |
| `working_capital` | `CurrentAssets − CurrentLiabilities` | Absolute value, not ratio |
| `free_cash_flow` | `OperatingCashFlow − CapEx` | Absolute value, not ratio |
| `fcf_margin` | `FreeCashFlow / Revenue` | |

---

## Graham defensive-investor scorecard

Implemented in `graham::assess()`. Based on Benjamin Graham's *The Intelligent Investor*, adapted for 10–15 years of XBRL data from EDGAR.

### 7 criteria

| # | Criterion | Threshold | Source |
|---|---|---|---|
| 1 | Adequate size | Revenue ≥ $500M (default floor, configurable) | `Revenue` |
| 2 | Strong financial position | Current ratio ≥ 2.0 | `CurrentAssets / CurrentLiabilities` |
| 3 | Earnings stability | Positive EPS in each of the last 3+ years | `Eps` annual |
| 4 | Dividend record | Uninterrupted dividends for 3+ years | `DividendPerShare` annual |
| 5 | Earnings growth | ≥33% EPS growth over 10 years (or available history) | `Eps` annual |
| 6 | Moderate P/E | P/E ≤ 15 | `pe` |
| 7 | Moderate P/B | P/B ≤ 1.5 (or P/E × P/B ≤ 22.5) | `pb`, `pe` |

### Score

`score` = number of criteria passed (0–7). `passes_defensive` = true when all 7 pass.

### Graham Number

```
Graham Number = √(22.5 × EPS × BookValuePerShare)
```

Represents the maximum price to pay per Graham. Only computed when both EPS and BVPS are positive. `22.5 = PE_MAX × PB_MAX = 15 × 1.5`.

### NCAV (Net Current Asset Value)

```
NCAV per share = (CurrentAssets − TotalLiabilities) / SharesOutstanding
```

A stock trading below NCAV is a "net-net" — buying the working capital for free. `net_net` = true when current price < ⅔ of NCAV per share.

### Margin of safety

```
Margin of safety = (GrahamNumber − Price) / GrahamNumber
```

Positive means the stock is trading below its Graham Number (undervalued). Negative means overvalued relative to intrinsic value.
