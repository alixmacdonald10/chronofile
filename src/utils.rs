use std::{
    io::{Error, ErrorKind},
    time::SystemTime,
};

use uuid::{Timestamp, Uuid};

pub fn convert_systemtime_to_uuid(time: SystemTime) -> std::io::Result<Uuid> {
    let timestamp = Timestamp::try_from(time).map_err(|err| {
        Error::new(
            ErrorKind::InvalidInput,
            "Failed to convert SystemTime to Timestamp",
        )
    })?;

    Ok(Uuid::new_v7(timestamp))
}
