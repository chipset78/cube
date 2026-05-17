use bollard::Docker;
use bollard::config::HostConfig;
use bollard::container::LogOutput;
use bollard::models::{RestartPolicy, RestartPolicyNameEnum};
use bollard::plugin::ContainerCreateBody;
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use futures_util::stream::TryStreamExt;
use std::io::{self, Write};

/// Конфигурация контейнера
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    pub name: Option<String>,
    pub attach_stdin: bool,
    pub attach_stdout: bool,
    pub attach_stderr: bool,
    pub exposed_ports: Option<Vec<String>>,
    pub cmd: Vec<String>,
    pub image: String,
    pub cpu: f64,
    pub memory: i64,
    pub disk: i64,
    pub env: Vec<String>,
    pub restart_policy: String,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            name: None,
            attach_stdin: false,
            attach_stdout: true,
            attach_stderr: true,
            exposed_ports: None,
            cmd: Vec::new(),
            image: String::new(),
            cpu: 0.5,
            memory: 512 * 1024 * 1024,     // 512 MB
            disk: 10 * 1024 * 1024 * 1024, // 10 GB
            env: Vec::new(),
            restart_policy: "no".to_string(),
        }
    }
}

impl ContainerConfig {
    pub fn new(image: String, name: String) -> Self {
        Self {
            image,
            name: Some(name),
            ..Default::default()
        }
    }

    pub fn with_env(mut self, env: Vec<String>) -> Self {
        self.env = env;
        self
    }

    pub fn with_cmd(mut self, cmd: Vec<String>) -> Self {
        self.cmd = cmd;
        self
    }

    pub fn with_resources(mut self, cpu: f64, memory: i64) -> Self {
        self.cpu = cpu;
        self.memory = memory;
        self
    }

    pub fn with_exposed_port(mut self, port: &str) -> Self {
        match &mut self.exposed_ports {
            Some(ports) => ports.push(port.to_string()),
            None => self.exposed_ports = Some(vec![port.to_string()]),
        }
        self
    }
}

/// Результат Docker операции
#[derive(Debug)]
pub struct DockerResult {
    pub error: Option<anyhow::Error>,
    pub action: String,
    pub container_id: String,
    pub result: String,
}

impl DockerResult {
    pub fn success(action: &str, container_id: &str) -> Self {
        DockerResult {
            error: None,
            action: action.to_string(),
            container_id: container_id.to_string(),
            result: "success".to_string(),
        }
    }

    pub fn failure(action: &str, error: anyhow::Error) -> Self {
        DockerResult {
            error: Some(error),
            action: action.to_string(),
            container_id: String::new(),
            result: "failure".to_string(),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// Docker клиент
pub struct DockerContainer {
    pub client: Docker,
    pub config: ContainerConfig,
}

impl DockerContainer {
    /// Создание нового Docker клиента
    pub async fn new(config: ContainerConfig) -> anyhow::Result<Self> {
        let client = Docker::connect_with_socket_defaults()?;
        Ok(DockerContainer { client, config })
    }

    /// Run - запуск контейнера
    pub async fn run(&mut self) -> DockerResult {
        println!("Pulling image: {}", self.config.image);
        if let Err(e) = self.pull_image().await {
            eprintln!("Error pulling image {}: {}", self.config.image, e);
            return DockerResult::failure("pull", e);
        }

        // Настройка политики перезапуска
        let restart_policy = match self.config.restart_policy.as_str() {
            "always" => RestartPolicyNameEnum::ALWAYS,
            "on-failure" => RestartPolicyNameEnum::ON_FAILURE,
            "unless-stopped" => RestartPolicyNameEnum::UNLESS_STOPPED,
            _ => RestartPolicyNameEnum::NO,
        };

        let restart_policy_config = RestartPolicy {
            name: Some(restart_policy),
            maximum_retry_count: None,
        };

        // Настройка ресурсов
        let nano_cpus = Some((self.config.cpu * 1_000_000_000.0) as i64);
        let memory = Some(self.config.memory);

        // Конфигурация хоста
        let host_config = HostConfig {
            restart_policy: Some(restart_policy_config),
            memory,
            nano_cpus,
            publish_all_ports: Some(true),
            ..Default::default()
        };

        // Конфигурация контейнера
        let container_config = ContainerCreateBody {
            image: Some(self.config.image.clone()),
            cmd: if self.config.cmd.is_empty() {
                None
            } else {
                Some(self.config.cmd.clone())
            },
            env: if self.config.env.is_empty() {
                None
            } else {
                Some(self.config.env.clone())
            },
            tty: Some(false),
            exposed_ports: self.config.exposed_ports.clone(),
            host_config: Some(host_config),
            ..Default::default()
        };

        let options = Some(CreateContainerOptions {
            name: self.config.name.clone(),
            platform: String::new(),
        });

        // Создание контейнера
        println!("Creating container using image {}", self.config.image);
        let container = match self
            .client
            .create_container(options, container_config)
            .await
        {
            Ok(container) => container,
            Err(e) => {
                eprintln!(
                    "Error creating container using image {}: {}",
                    self.config.image, e
                );
                return DockerResult::failure("create", e.into());
            }
        };

        let container_id = container.id;

        // Запуск контейнера
        println!("Starting container {}", container_id);
        if let Err(e) = self
            .client
            .start_container(&container_id, None::<StartContainerOptions>)
            .await
        {
            eprintln!("Error starting container {}: {}", container_id, e);
            return DockerResult::failure("start", e.into());
        }

        // Получение логов
        if let Err(e) = self.print_logs(&container_id).await {
            eprintln!("Error getting logs for container {}: {}", container_id, e);
            // Не возвращаем ошибку, логирование не критично
        }

        DockerResult::success("start", &container_id)
    }

    /// Stop - остановка и удаление контейнера
    pub async fn stop(&self, id: &str) -> DockerResult {
        println!("Attempting to stop container {}", id);

        // Остановка контейнера
        let stop_options = StopContainerOptions::default();
        if let Err(e) = self.client.stop_container(id, Some(stop_options)).await {
            eprintln!("Error stopping container {}: {}", id, e);
            return DockerResult::failure("stop", e.into());
        }

        // Удаление контейнера
        let remove_options = Some(RemoveContainerOptions {
            v: true, // RemoveVolumes
            force: false,
            link: false,
        });

        if let Err(e) = self.client.remove_container(id, remove_options).await {
            eprintln!("Error removing container {}: {}", id, e);
            return DockerResult::failure("remove", e.into());
        }

        DockerResult::success("stop", id)
    }

    /// Pull образа
    async fn pull_image(&self) -> anyhow::Result<()> {
        let options = Some(CreateImageOptions {
            from_image: Some(self.config.image.clone()),
            ..Default::default()
        });

        let mut stream = self.client.create_image(options, None, None);

        while let Some(result) = stream.try_next().await? {
            if let Some(error) = result.error_detail {
                anyhow::bail!("Docker pull error: {:?}", error);
            }
            if let Some(status) = result.status {
                println!("Pull status: {}", status);
            }
        }

        Ok(())
    }

    /// Печать логов контейнера
    async fn print_logs(&self, container_id: &str) -> anyhow::Result<()> {
        let logs_options = LogsOptions {
            follow: false,
            stdout: true,
            stderr: true,
            timestamps: false,
            since: 0,
            until: 0,
            tail: String::new(),
        };

        let mut logs_stream = self.client.logs(container_id, Some(logs_options));

        while let Some(log_result) = logs_stream.try_next().await? {
            match log_result {
                LogOutput::StdOut { message } => {
                    io::stdout().write_all(&message)?;
                    io::stdout().flush()?;
                }
                LogOutput::StdErr { message } => {
                    io::stderr().write_all(&message)?;
                    io::stderr().flush()?;
                }
                _ => {}
            }
        }

        Ok(())
    }
}

/// Функция создания контейнера
#[allow(dead_code)]
pub async fn create_container() -> anyhow::Result<(DockerContainer, DockerResult)> {
    let config = ContainerConfig::new("postgres:16".to_string(), "test-container-1".to_string())
        .with_env(vec![
            "POSTGRES_USER=cube".to_string(),
            "POSTGRES_PASSWORD=secret".to_string(),
        ]);

    let mut docker = DockerContainer::new(config).await?;
    let result = docker.run().await;

    if result.error.is_some() {
        anyhow::bail!("Failed to create container: {:?}", result.error);
    }

    println!("Container {} is running with config", result.container_id);
    Ok((docker, result))
}

/// Функция остановки контейнера
#[allow(dead_code)]
pub async fn stop_container(docker: &DockerContainer, id: &str) -> Option<DockerResult> {
    let result = docker.stop(id).await;
    if result.error.is_some() {
        println!("Error stopping container: {:?}", result.error);
        return None;
    }
    println!(
        "Container {} has been stopped and removed",
        result.container_id
    );
    Some(result)
}
