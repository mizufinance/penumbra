use anyhow::Result;
use comfy_table::{presets, Table};
use penumbra_sdk_view::{SpendableNoteRecord, ViewClient};

#[derive(Debug, clap::Args)]
pub struct NotesCmd {
    /// Restrict results to notes received by this account.
    #[clap(long, default_value = "0")]
    pub account: u32,
    /// Restrict results to a single asset denom or unit, e.g. regulated_usd.
    #[clap(long)]
    pub asset: Option<String>,
    /// Show only the largest note in the selected result set.
    #[clap(long)]
    pub largest: bool,
    /// Print only note commitments, one per line.
    #[clap(long)]
    pub commitment_only: bool,
}

struct NoteRow {
    asset_label: String,
    value_label: String,
    amount: penumbra_sdk_num::Amount,
    note_commitment_hex: String,
    record: SpendableNoteRecord,
}

impl NotesCmd {
    pub fn offline(&self) -> bool {
        false
    }

    pub async fn exec<V: ViewClient>(&self, view: &mut V) -> Result<()> {
        let asset_cache = view.assets().await?;
        let notes = view.unspent_notes_by_account_and_asset().await?;

        let mut rows = Vec::new();
        if let Some(notes_by_asset) = notes.get(&self.account) {
            for (asset_id, records) in notes_by_asset {
                let metadata = asset_cache.get(asset_id).cloned();
                for record in records.iter().cloned() {
                    let asset_label = metadata
                        .as_ref()
                        .map(|m| m.base_denom().denom)
                        .unwrap_or_else(|| asset_id.to_string());
                    let value_label = metadata
                        .as_ref()
                        .map(|m| m.value(record.note.amount()).format(&asset_cache))
                        .unwrap_or_else(|| format!("{} {}", record.note.amount(), asset_id));
                    rows.push(NoteRow {
                        asset_label,
                        value_label,
                        amount: record.note.amount(),
                        note_commitment_hex: hex::encode(record.note_commitment.0.to_bytes()),
                        record,
                    });
                }
            }
        }

        if let Some(asset_filter) = &self.asset {
            rows.retain(|row| note_matches_asset_filter(row, asset_filter));
        }

        rows.sort_by(|a, b| {
            a.asset_label
                .cmp(&b.asset_label)
                .then_with(|| b.amount.cmp(&a.amount))
                .then_with(|| a.note_commitment_hex.cmp(&b.note_commitment_hex))
        });

        if self.largest {
            rows = rows
                .into_iter()
                .max_by(|a, b| {
                    a.amount
                        .cmp(&b.amount)
                        .then_with(|| a.note_commitment_hex.cmp(&b.note_commitment_hex))
                })
                .into_iter()
                .collect();
        }

        if rows.is_empty() {
            let scope = self.asset.as_deref().unwrap_or("any asset");
            anyhow::bail!(
                "no unspent notes found for account {} and asset filter {}",
                self.account,
                scope
            );
        }

        if self.commitment_only {
            for row in &rows {
                println!("{}", row.note_commitment_hex);
            }
            return Ok(());
        }

        let mut table = Table::new();
        table.load_preset(presets::NOTHING);
        table.set_header(vec!["Account", "Asset", "Value", "Commitment", "Position"]);

        for row in rows {
            table.add_row(vec![
                format!("# {}", self.account),
                row.asset_label,
                row.value_label,
                row.note_commitment_hex,
                format!("{:?}", row.record.position),
            ]);
        }

        println!("{table}");
        Ok(())
    }
}

fn note_matches_asset_filter(row: &NoteRow, asset_filter: &str) -> bool {
    row.asset_label == asset_filter || row.value_label.ends_with(asset_filter)
}
