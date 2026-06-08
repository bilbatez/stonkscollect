import { Chip, Link, List, ListItem, Stack, Typography } from '@mui/material'
import type { NewsItem } from '../types'

/** Newest-first list of headlines (title + description only). */
export function NewsFeed({ news }: { news: NewsItem[] }) {
  if (news.length === 0) {
    return <Typography color="text.secondary">No news.</Typography>
  }
  return (
    <List disablePadding>
      {news.map((n) => (
        <ListItem key={n.dedup_hash} divider alignItems="flex-start" disableGutters>
          <Stack spacing={0.5}>
            <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
              <Link href={n.url} target="_blank" rel="noreferrer" underline="hover">
                {n.title}
              </Link>
              <Chip size="small" variant="outlined" label={n.source} />
            </Stack>
            {n.description !== null && (
              <Typography variant="body2" color="text.secondary">
                {n.description}
              </Typography>
            )}
          </Stack>
        </ListItem>
      ))}
    </List>
  )
}
