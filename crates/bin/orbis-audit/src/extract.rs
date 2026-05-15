use penumbra_sdk_compliance::{TransferComplianceCiphertext, TransferOrbisUploadBundle};
use penumbra_sdk_proto::core::transaction::v1::{action::Action as ActionEnum, Action};

#[derive(Debug)]
pub struct ExtractedTransferData {
    pub ciphertext: TransferComplianceCiphertext,
    pub bundle: TransferOrbisUploadBundle,
}

#[derive(Debug)]
pub enum TransferExtraction {
    Found(ExtractedTransferData),
    Skipped(ExtractionSkip),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractionSkip {
    NonTransferAction,
    MissingTransferBody,
    MissingOutput,
    EmptyComplianceCiphertext,
    InvalidComplianceCiphertext { len: usize, error: String },
    InvalidUploadBundle { len: usize, error: String },
}

impl ExtractionSkip {
    pub fn is_malformed(&self) -> bool {
        matches!(
            self,
            Self::InvalidComplianceCiphertext { .. } | Self::InvalidUploadBundle { .. }
        )
    }
}

pub fn extract_transfer_data(action: &Action, output_index: usize) -> TransferExtraction {
    let Some(action) = action.action.as_ref() else {
        return TransferExtraction::Skipped(ExtractionSkip::NonTransferAction);
    };
    let ActionEnum::Transfer(transfer) = action else {
        return TransferExtraction::Skipped(ExtractionSkip::NonTransferAction);
    };
    let Some(body) = transfer.body.as_ref() else {
        return TransferExtraction::Skipped(ExtractionSkip::MissingTransferBody);
    };
    let Some(output) = body.outputs.get(output_index) else {
        return TransferExtraction::Skipped(ExtractionSkip::MissingOutput);
    };
    if output.compliance_ciphertext.is_empty() {
        return TransferExtraction::Skipped(ExtractionSkip::EmptyComplianceCiphertext);
    }

    let ciphertext = match TransferComplianceCiphertext::from_bytes(&output.compliance_ciphertext) {
        Ok(ciphertext) => ciphertext,
        Err(error) => {
            return TransferExtraction::Skipped(ExtractionSkip::InvalidComplianceCiphertext {
                len: output.compliance_ciphertext.len(),
                error: error.to_string(),
            });
        }
    };
    let bundle = match TransferOrbisUploadBundle::from_bytes(&output.orbis_upload_bundle) {
        Ok(bundle) => bundle,
        Err(error) => {
            return TransferExtraction::Skipped(ExtractionSkip::InvalidUploadBundle {
                len: output.orbis_upload_bundle.len(),
                error: error.to_string(),
            });
        }
    };
    TransferExtraction::Found(ExtractedTransferData { ciphertext, bundle })
}
