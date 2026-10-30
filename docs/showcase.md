# Showcase

The README screenshot is the headline-terms table for the 44 ISDA FpML 5.12
credit-derivatives example documents shipped under
`schemas/confirmation/examples/credit-derivatives`.

| File | What it shows |
| --- | --- |
| `docs/showcase/cds-summary.txt` | `cds_summary` example output: reference entity, RED, notional, currency, coupon, effective and maturity dates per example |
| `docs/showcase/cds-summary.png` | The same output rendered with charm `freeze` |

Reproduce both:

```bash
docs/showcase/make-screenshots.sh
```

The PNG render is:

```bash
freeze docs/showcase/cds-summary.txt --language text --window --padding 20 --font.size 14 \
  --output docs/showcase/cds-summary.png
sips -Z 1800 docs/showcase/cds-summary.png
pngquant --force --quality 50-75 --output docs/showcase/cds-summary.png docs/showcase/cds-summary.png
```

The last two commands shrink the render below 300 KB for the README.
