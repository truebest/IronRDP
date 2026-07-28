use ironrdp_core::{encode_vec, impl_as_any};
use ironrdp_dvc::{DrdynvcClient, DvcChannelListener, DvcMessage, DvcProcessor, DynamicChannelId};
use ironrdp_pdu::PduResult;
use ironrdp_svc::SvcProcessor as _;

use super::*;

const TEST_CHANNEL: &str = "Test::Channel";

struct TestProcessor;

impl_as_any!(TestProcessor);

impl DvcProcessor for TestProcessor {
    fn channel_name(&self) -> &str {
        TEST_CHANNEL
    }

    fn start(&mut self, _channel_id: u32) -> PduResult<Vec<DvcMessage>> {
        Ok(Vec::new())
    }

    fn process(&mut self, _channel_id: u32, _payload: &[u8]) -> PduResult<Vec<DvcMessage>> {
        Ok(Vec::new())
    }
}

struct TestListener;

impl DvcChannelListener for TestListener {
    fn channel_name(&self) -> &str {
        TEST_CHANNEL
    }

    fn create(&mut self, _channel_id: DynamicChannelId) -> Option<Box<dyn DvcProcessor>> {
        Some(Box::new(TestProcessor))
    }
}

fn process_server_pdu(client: &mut DrdynvcClient, pdu: DrdynvcServerPdu) {
    let bytes = encode_vec(&pdu).expect("encode server PDU");
    client.process(&bytes).expect("process server PDU");
}

fn client_with_caps_done(mut client: DrdynvcClient) -> DrdynvcClient {
    process_server_pdu(
        &mut client,
        DrdynvcServerPdu::Capabilities(CapabilitiesRequestPdu::new(CapsVersion::V1, None)),
    );
    client
}

#[test]
fn typed_listener_survives_server_channel_recreate() {
    let mut client =
        client_with_caps_done(DrdynvcClient::new().with_typed_listener::<TestProcessor, _>(TestListener));

    process_server_pdu(
        &mut client,
        DrdynvcServerPdu::Create(CreateRequestPdu::new(7, TEST_CHANNEL.to_owned())),
    );
    let dvc = client
        .get_dvc_by_type_id::<TestProcessor>()
        .expect("channel after create");
    assert_eq!(dvc.channel_id(), Some(7));

    process_server_pdu(&mut client, DrdynvcServerPdu::Close(ClosePdu::new(7)));
    assert!(client.get_dvc_by_type_id::<TestProcessor>().is_none());

    process_server_pdu(
        &mut client,
        DrdynvcServerPdu::Create(CreateRequestPdu::new(9, TEST_CHANNEL.to_owned())),
    );
    let dvc = client
        .get_dvc_by_type_id::<TestProcessor>()
        .expect("channel after re-create");
    assert_eq!(dvc.channel_id(), Some(9));
    assert!(dvc.channel_processor_downcast_ref::<TestProcessor>().is_some());
}

#[test]
fn once_registered_channel_is_lost_after_server_channel_recreate() {
    let mut client = client_with_caps_done(DrdynvcClient::new().with_dynamic_channel(TestProcessor));

    process_server_pdu(
        &mut client,
        DrdynvcServerPdu::Create(CreateRequestPdu::new(7, TEST_CHANNEL.to_owned())),
    );
    assert!(client.get_dvc_by_type_id::<TestProcessor>().is_some());

    process_server_pdu(&mut client, DrdynvcServerPdu::Close(ClosePdu::new(7)));
    process_server_pdu(
        &mut client,
        DrdynvcServerPdu::Create(CreateRequestPdu::new(9, TEST_CHANNEL.to_owned())),
    );
    assert!(
        client.get_dvc_by_type_id::<TestProcessor>().is_none(),
        "once-registered processors are consumed by the first create; \
         use with_typed_listener for channels the server may re-create"
    );
}
