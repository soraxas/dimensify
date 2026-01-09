use pyo3::{exceptions::PyValueError, prelude::*};
use std::{fs::OpenOptions, io::Write};

use dimensify_protocol::{TelemetryEvent, TelemetryMetadata, TelemetryPayload, TelemetryTime};

/// Append-only telemetry logger (JSONL).
#[pyclass]
pub struct TelemetryClient {
    path: String,
}

#[pymethods]
impl TelemetryClient {
    #[new]
    pub fn new(path: String) -> PyResult<Self> {
        if path.is_empty() {
            return Err(PyValueError::new_err("telemetry path cannot be empty"));
        }
        Ok(Self { path })
    }

    /// Log a scalar value to a telemetry path.
    #[pyo3(signature = (path, time, value, timeline=None, unit=None, description=None))]
    pub fn log_scalar(
        &self,
        path: String,
        time: f64,
        value: f64,
        timeline: Option<String>,
        unit: Option<String>,
        description: Option<String>,
    ) -> PyResult<()> {
        self.write_event(
            path,
            time,
            timeline,
            TelemetryPayload::Scalar { value },
            unit,
            description,
        )
    }

    /// Log a Vec3 value to a telemetry path.
    #[pyo3(signature = (path, time, value, timeline=None, unit=None, description=None))]
    pub fn log_vec3(
        &self,
        path: String,
        time: f64,
        value: (f32, f32, f32),
        timeline: Option<String>,
        unit: Option<String>,
        description: Option<String>,
    ) -> PyResult<()> {
        self.write_event(
            path,
            time,
            timeline,
            TelemetryPayload::Vec3 {
                value: dimensify_protocol::prelude::Vec3::from([value.0, value.1, value.2]),
            },
            unit,
            description,
        )
    }

    /// Log a text value to a telemetry path.
    #[pyo3(signature = (path, time, value, timeline=None, unit=None, description=None))]
    pub fn log_text(
        &self,
        path: String,
        time: f64,
        value: String,
        timeline: Option<String>,
        unit: Option<String>,
        description: Option<String>,
    ) -> PyResult<()> {
        self.write_event(
            path,
            time,
            timeline,
            TelemetryPayload::Text { value },
            unit,
            description,
        )
    }
}

impl TelemetryClient {
    fn write_event(
        &self,
        path: String,
        time: f64,
        timeline: Option<String>,
        payload: TelemetryPayload,
        unit: Option<String>,
        description: Option<String>,
    ) -> PyResult<()> {
        let event = TelemetryEvent {
            path,
            time: TelemetryTime {
                timeline: timeline.unwrap_or_else(|| "sim_time".to_string()),
                value: time,
            },
            payload,
            metadata: Some(TelemetryMetadata { unit, description }),
        };
        let line =
            serde_json::to_string(&event).map_err(|err| PyValueError::new_err(err.to_string()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| PyValueError::new_err(err.to_string()))?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }
}
