use alloc::vec::Vec;

use ironrdp_core::{DecodeResult, cast_length, invalid_field_err};

use crate::pdu::DrdynvcDataPdu;

#[derive(Debug, PartialEq)]
pub(crate) struct CompleteData {
    total_size: usize,
    data: Vec<u8>,
}

impl CompleteData {
    pub(crate) fn new() -> Self {
        Self {
            total_size: 0,
            data: Vec::new(),
        }
    }

    pub(crate) fn process_data(&mut self, pdu: DrdynvcDataPdu, maximum_size: usize) -> DecodeResult<Option<Vec<u8>>> {
        let result = match pdu {
            DrdynvcDataPdu::DataFirst(data) => {
                self.total_size = 0;
                self.data.clear();
                let total_size: usize = cast_length!("DataFirstPdu::length", data.length())?;
                if total_size > maximum_size || data.data().len() > total_size {
                    Err(invalid_field_err!(
                        "DVC message",
                        "length",
                        "message exceeds its size limit"
                    ))
                } else if total_size == data.data().len() {
                    Ok(Some(data.into_data()))
                } else {
                    self.total_size = total_size;
                    self.data = data.into_data();
                    Ok(None)
                }
            }
            DrdynvcDataPdu::Data(data) => {
                if self.total_size == 0 {
                    if data.data().len() > maximum_size {
                        Err(invalid_field_err!(
                            "DVC message",
                            "data",
                            "message exceeds its size limit"
                        ))
                    } else {
                        Ok(Some(data.into_data()))
                    }
                } else if self
                    .data
                    .len()
                    .checked_add(data.data().len())
                    .is_none_or(|length| length > self.total_size || length > maximum_size)
                {
                    Err(invalid_field_err!(
                        "DVC message",
                        "data",
                        "fragments exceed the message size limit"
                    ))
                } else {
                    self.data.extend_from_slice(data.data());
                    if self.data.len() == self.total_size {
                        self.total_size = 0;
                        Ok(Some(core::mem::take(&mut self.data)))
                    } else {
                        Ok(None)
                    }
                }
            }
        };
        if result.is_err() {
            self.total_size = 0;
            self.data.clear();
        }
        result
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
