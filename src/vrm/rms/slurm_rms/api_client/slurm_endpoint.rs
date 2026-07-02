#[derive(Debug)]
pub enum SlurmEndpoint {
    Nodes,
    Jobs,
    Job,
    Config,
    JobSubmit,
    Ping,
}

impl SlurmEndpoint {
    pub fn path(&self) -> &str {
        match self {
            Self::Nodes => "/nodes",
            Self::Jobs => "/jobs",
            Self::Job => "/job",
            Self::Config => "/config",
            Self::JobSubmit => "/job/submit",
            Self::Ping => "/ping",
        }
    }
}
