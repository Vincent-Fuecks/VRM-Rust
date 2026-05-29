use std::{collections::HashSet, sync::Arc};

use vrm_rust_workflow::{
    api::{
        rms_config_dto::rms_dto::{RmsSystemWrapper, SlurmConfigDto, SlurmRmsDto, SwitchDto},
        vrm_system_model_dto::aci_dto::AcIDto,
    },
    domain::{
        simulator::simulator::GlobalClock,
        vrm_system_model::{grid_resource_management_system::aci::AcI, reservation::reservation_store::ReservationStore},
    },
};

async fn create_slurm_rms_mock() -> Result<SlurmRmsDto, Box<dyn std::error::Error>> {
    let rest_api_config: SlurmConfigDto = SlurmConfigDto {
        base_url: "http://localhost:6820".to_string(),
        version: "v0.0.41".to_string(),
        user_name: "root".to_string(),
        jwt_token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIwOTE1MDYxNDQsImlhdCI6MTc3NjE0NjE0NCwic3VuIjoic2x1cm0ifQ.1c0D0fH2bP9MS3qmwf944xH9894r_aeaHFgnGaMYw-Q".to_string()
    };

    // Define the individual switches for the topology
    let slurm_switch_dto_0 =
        SwitchDto { switch_name: "s0".to_string(), switches: vec![], nodes: vec!["c0".to_string(), "c1".to_string()], link_speed: 1000 };

    let slurm_switch_dto_1 = SwitchDto {
        switch_name: "s1".to_string(),
        switches: vec![],
        nodes: vec!["c3".to_string(), "c4".to_string(), "c5".to_string(), "c6".to_string()],
        link_speed: 1000,
    };

    let slurm_switch_dto_2 = SwitchDto {
        switch_name: "s2".to_string(),
        switches: vec!["s0".to_string(), "s1".to_string()],
        nodes: vec!["c2".to_string()],
        link_speed: 1000,
    };

    // 3. Assemble the topology vector
    let topology: Vec<SwitchDto> = vec![slurm_switch_dto_0, slurm_switch_dto_1, slurm_switch_dto_2];

    return Ok(SlurmRmsDto {
        id: "RMS-ID".to_string(),
        scheduler_typ: "SlottedSchedule".to_string(),
        slot_width: 60,
        num_of_slots: 60,
        rest_api_config: rest_api_config,
        entry_points: vec!["s0".to_string()],
        topology: topology,
    });
}

pub async fn create_aci_with_slurm_rms() -> Result<AcI, Box<dyn std::error::Error>> {
    let simulator = Arc::new(GlobalClock::new(false));
    let reservation_store = ReservationStore::new();

    let rms_system = create_slurm_rms_mock().await?;
    let aci_dto =
        AcIDto { id: "Test-AcI".to_string(), adc_id: "Master-ADC".to_string(), commit_timeout: 10, rms_system: RmsSystemWrapper::Slurm(rms_system) };

    let aci = AcI::from_dto(aci_dto, simulator, reservation_store).await?;
    return Ok(aci);
}
