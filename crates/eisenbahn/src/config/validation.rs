use super::helpers::topological_sort;
use super::types::EisenbahnConfig;
use crate::error::EisenbahnError;

impl EisenbahnConfig {
    /// Validate the config: check for circular dependencies, missing references, etc.
    pub fn validate(&self) -> Result<(), EisenbahnError> {
        self.validate_pipeline_references()?;
        self.validate_no_circular_dependencies()?;
        self.validate_worker_pipelines()?;
        self.validate_transport_kind()?;
        Ok(())
    }

    /// Ensure all `after` references in pipeline stages point to existing stages.
    fn validate_pipeline_references(&self) -> Result<(), EisenbahnError> {
        for (name, stage) in &self.pipeline.stages {
            for dep in &stage.after {
                if !self.pipeline.stages.contains_key(dep) {
                    return Err(EisenbahnError::Config(format!(
                        "pipeline stage '{name}' references unknown upstream stage '{dep}'"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Detect circular dependencies in the pipeline DAG.
    fn validate_no_circular_dependencies(&self) -> Result<(), EisenbahnError> {
        topological_sort(&self.pipeline.stages)?;
        Ok(())
    }

    /// Ensure worker pipeline references point to existing stages.
    fn validate_worker_pipelines(&self) -> Result<(), EisenbahnError> {
        for (name, worker) in &self.workers {
            for pipeline in &worker.pipelines {
                if !self.pipeline.stages.is_empty()
                    && !self.pipeline.stages.contains_key(pipeline)
                {
                    return Err(EisenbahnError::Config(format!(
                        "worker '{name}' references unknown pipeline stage '{pipeline}'"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Ensure transport kind is valid.
    fn validate_transport_kind(&self) -> Result<(), EisenbahnError> {
        match self.transport.kind.as_str() {
            "ipc" | "tcp" => Ok(()),
            other => Err(EisenbahnError::Config(format!(
                "invalid transport kind '{other}', expected 'ipc' or 'tcp'"
            ))),
        }
    }
}
