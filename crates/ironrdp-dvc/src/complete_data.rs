use alloc::vec::Vec;

use ironrdp_bulk::rdp8::Rdp8Decompressor;
use ironrdp_core::{DecodeResult, cast_length, invalid_field_err, other_err};

use crate::pdu::DrdynvcDataPdu;

#[derive(Debug)]
pub(crate) struct CompleteData {
    total_size: usize,
    data: Vec<u8>,
    /// MS-RDPEDYC 3.1.5.2.5: each channel has its own decompression context, created when
    /// the channel first receives compressed data.
    decompressor: Option<Rdp8Decompressor>,
    /// Set once decompression fails. The history may have advanced partway through the bad
    /// block, so it no longer matches the sender's and cannot decode anything after it.
    decompression_failed: bool,
}

impl CompleteData {
    pub(crate) fn new() -> Self {
        Self {
            total_size: 0,
            data: Vec::new(),
            decompressor: None,
            decompression_failed: false,
        }
    }

    pub(crate) fn process_data(&mut self, pdu: DrdynvcDataPdu, maximum_size: usize) -> DecodeResult<Option<Vec<u8>>> {
        let result = match pdu {
            DrdynvcDataPdu::DataFirst(data) => self.process_data_first(data.length(), data.into_data(), maximum_size),
            DrdynvcDataPdu::Data(data) => self.process_data_block(data.into_data(), maximum_size),
            DrdynvcDataPdu::DataFirstCompressed(data) => self
                .decompress(data.data())
                .and_then(|bytes| self.process_data_first(data.length(), bytes, maximum_size)),
            DrdynvcDataPdu::DataCompressed(data) => self
                .decompress(data.data())
                .and_then(|bytes| self.process_data_block(bytes, maximum_size)),
        };
        if result.is_err() {
            self.total_size = 0;
            self.data.clear();
        }
        result
    }

    fn decompress(&mut self, compressed: &[u8]) -> DecodeResult<Vec<u8>> {
        if self.decompression_failed {
            return Err(invalid_field_err!(
                "DVC compressed data",
                "data",
                "the channel's decompression history is out of sync after an earlier error"
            ));
        }

        let decompressor = self.decompressor.get_or_insert_with(Rdp8Decompressor::new_lite);
        let mut data = Vec::new();
        if let Err(e) = decompressor.decompress(compressed, &mut data) {
            self.decompression_failed = true;
            return Err(other_err!("DVC compressed data", source: e));
        }
        Ok(data)
    }

    fn process_data_first(&mut self, total_size: u32, data: Vec<u8>, maximum_size: usize) -> DecodeResult<Option<Vec<u8>>> {
        self.total_size = 0;
        self.data.clear();
        let total_size: usize = cast_length!("DataFirstPdu::length", total_size)?;
        if total_size > maximum_size || data.len() > total_size {
            Err(invalid_field_err!("DVC message", "length", "message exceeds its size limit"))
        } else if total_size == data.len() {
            Ok(Some(data))
        } else {
            self.total_size = total_size;
            self.data = data;
            Ok(None)
        }
    }

    fn process_data_block(&mut self, data: Vec<u8>, maximum_size: usize) -> DecodeResult<Option<Vec<u8>>> {
        if self.total_size == 0 {
            if data.len() > maximum_size {
                Err(invalid_field_err!("DVC message", "data", "message exceeds its size limit"))
            } else {
                Ok(Some(data))
            }
        } else if self.data.len().checked_add(data.len()).is_none_or(|length| length > self.total_size || length > maximum_size) {
            Err(invalid_field_err!("DVC message", "data", "fragments exceed the message size limit"))
        } else {
            self.data.extend_from_slice(&data);
            if self.data.len() == self.total_size {
                self.total_size = 0;
                Ok(Some(core::mem::take(&mut self.data)))
            } else {
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdu::{DataFirstPdu, DataPdu};
    use alloc::vec;

    fn first(length: u32, bytes: &[u8]) -> DrdynvcDataPdu {
        DrdynvcDataPdu::DataFirst(DataFirstPdu::new(1, length, bytes.to_vec()))
    }

    fn data(bytes: &[u8]) -> DrdynvcDataPdu {
        DrdynvcDataPdu::Data(DataPdu::new(1, bytes.to_vec()))
    }

    #[test]
    fn rejects_oversize_before_retaining_fragments_and_resets() {
        let mut accumulator = CompleteData::new();
        assert!(accumulator.process_data(first(u32::MAX, &[1]), 16).is_err());
        assert!(accumulator.data.is_empty());
        assert_eq!(accumulator.total_size, 0);
        assert!(accumulator.process_data(first(3, &[1, 2, 3, 4]), 16).is_err());
        assert!(accumulator.process_data(data(&[0; 17]), 16).is_err());
        assert_eq!(accumulator.process_data(first(4, &[1, 2]), 16).unwrap(), None);
        assert!(accumulator.process_data(data(&[3, 4, 5]), 16).is_err());
        assert!(accumulator.data.is_empty());
        assert_eq!(accumulator.process_data(data(&[9]), 16).unwrap(), Some(vec![9]));
    }

    #[test]
    fn reassembles_valid_data_with_channel_specific_graphics_budget() {
        let mut accumulator = CompleteData::new();
        assert_eq!(accumulator.process_data(first(4, &[1, 2]), 4).unwrap(), None);
        assert_eq!(
            accumulator.process_data(data(&[3, 4]), 4).unwrap(),
            Some(vec![1, 2, 3, 4])
        );
        let large_bitmap = 3840 * 2160 * 4;
        assert!(
            accumulator
                .process_data(first(large_bitmap, &[1]), 16 * 1024 * 1024)
                .is_err()
        );
        assert_eq!(
            accumulator
                .process_data(first(large_bitmap, &[1]), 64 * 1024 * 1024)
                .unwrap(),
            None
        );
        assert_eq!(accumulator.data, [1]);
    }
}
