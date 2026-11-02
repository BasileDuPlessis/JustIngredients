# Dashboards for Fly's managed Grafana

Plan O6 of [specs/tasks/observability.md](../../specs/tasks/observability.md). Fly collects the
bot's `ji_*` metrics from its internal port (`[metrics]` in `deploy/fly.toml`) and keeps them
about 15 days; these three dashboards show them on [fly-metrics.net](https://fly-metrics.net),
with Fly's own machine and Postgres metrics.

| File | Dashboard | Answers |
|---|---|---|
| `usage.json` | JustIngredients · Usage | Is the bot used? Users, photos, recipes, actions |
| `gemini.json` | JustIngredients · Gemini et coûts | What does Gemini cost? Calls, tokens, cost, photo read time |
| `health.json` | JustIngredients · Santé | Does it work, and fast? Errors, breaker, restarts, speed, memory, CPU, database |

Import (once, and again after a change): fly-metrics.net → Dashboards → New → Import → upload
the file, choose the Prometheus data source ("Prometheus on Fly"). Importing a file again with
the same `uid` replaces the dashboard. The `Bot` and `Base` selectors switch between production
and staging.

The JSON is generated: edit and run the generator kept with the plan rather than the JSON by
hand, so that the three keep the same palette (the reference palette's dark steps, in fixed
order; status colors only for the breaker), 2 px lines, daily bars and legends.
