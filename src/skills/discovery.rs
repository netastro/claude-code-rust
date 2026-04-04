//! 技能发现模块
//!
//! 实现技能自动发现和扫描功能

use std::path::PathBuf;
use std::collections::HashSet;
use crate::error::Result;
use super::registry::SkillRegistry;
use super::types::SkillMetadata;

/// 技能发现器
pub struct SkillDiscovery {
    /// 已扫描的路径
    scanned_paths: HashSet<PathBuf>,

    /// 排除的路径模式
    exclude_patterns: Vec<String>,
}

impl SkillDiscovery {
    /// 创建新的技能发现器
    pub fn new() -> Self {
        Self {
            scanned_paths: HashSet::new(),
            exclude_patterns: vec![
                ".*".to_string(),      // 隐藏文件
                "node_modules".to_string(),
                "target".to_string(),
                "build".to_string(),
                "dist".to_string(),
                ".git".to_string(),
            ],
        }
    }

    /// 添加排除模式
    pub fn add_exclude_pattern(&mut self, pattern: String) {
        self.exclude_patterns.push(pattern);
    }

    /// 扫描目录中的技能
    pub async fn scan_directory(&mut self, dir: &PathBuf, registry: &SkillRegistry) -> Result<Vec<SkillMetadata>> {
        // 检查是否已扫描
        if self.scanned_paths.contains(dir) {
            return Ok(Vec::new());
        }

        // 检查是否应该排除
        if self.should_exclude(dir) {
            tracing::debug!("跳过排除目录: {:?}", dir);
            return Ok(Vec::new());
        }

        // 标记为已扫描
        self.scanned_paths.insert(dir.clone());

        let mut discovered_skills = Vec::new();

        // 扫描目录
        match self.scan_directory_internal(dir, registry).await {
            Ok(skills) => {
                discovered_skills.extend(skills);
                tracing::info!("在目录 {:?} 中发现 {} 个技能", dir, discovered_skills.len());
            }
            Err(e) => {
                tracing::warn!("扫描目录 {:?} 失败: {}", dir, e);
            }
        }

        Ok(discovered_skills)
    }

    /// 扫描多个目录
    pub async fn scan_directories(
        &mut self,
        dirs: &[PathBuf],
        registry: &SkillRegistry,
    ) -> Result<Vec<SkillMetadata>> {
        let mut all_skills = Vec::new();

        for dir in dirs {
            let skills = self.scan_directory(dir, registry).await?;
            all_skills.extend(skills);
        }

        Ok(all_skills)
    }

    /// 扫描默认位置
    pub async fn scan_default_locations(&mut self, registry: &SkillRegistry) -> Result<Vec<SkillMetadata>> {
        let default_dirs = self.get_default_search_directories();
        self.scan_directories(&default_dirs, registry).await
    }

    /// 获取默认搜索目录
    pub fn get_default_search_directories(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // 用户目录
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".claude-code").join("skills"));
            dirs.push(home.join(".config").join("claude-code").join("skills"));
        }

        // 当前目录
        dirs.push(PathBuf::from("./.claude/skills"));
        dirs.push(PathBuf::from("./skills"));
        dirs.push(PathBuf::from("./.skills"));

        // 系统目录
        if cfg!(unix) {
            dirs.push(PathBuf::from("/usr/local/share/claude-code/skills"));
            dirs.push(PathBuf::from("/usr/share/claude-code/skills"));
        }

        // 过滤掉不存在的目录
        dirs.into_iter()
            .filter(|dir| dir.exists())
            .collect()
    }

    /// 检查是否应该排除路径
    fn should_exclude(&self, path: &PathBuf) -> bool {
        let path_str = path.to_string_lossy().to_string();

        for pattern in &self.exclude_patterns {
            // 简单的字符串包含匹配
            if pattern.starts_with('*') && pattern.ends_with('*') {
                let pat = &pattern[1..pattern.len()-1];
                if path_str.contains(pat) {
                    return true;
                }
            } else if pattern.starts_with('*') {
                let pat = &pattern[1..];
                if path_str.ends_with(pat) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                let pat = &pattern[..pattern.len()-1];
                if path_str.starts_with(pat) {
                    return true;
                }
            } else if path_str == pattern {
                return true;
            }
        }

        false
    }

    /// 内部目录扫描实现（迭代版本，避免递归future）
    async fn scan_directory_internal(
        &self,
        dir: &PathBuf,
        _registry: &SkillRegistry,
    ) -> Result<Vec<SkillMetadata>> {
        let mut skills_found = Vec::new();
        let mut directories_to_scan = vec![dir.clone()];

        while let Some(current_dir) = directories_to_scan.pop() {
            // 读取目录内容
            let entries = match std::fs::read_dir(&current_dir) {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!("无法读取目录 {:?}: {}", current_dir, e);
                    continue;
                }
            };

            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(e) => {
                        tracing::debug!("无法读取目录条目: {}", e);
                        continue;
                    }
                };

                let path = entry.path();

                // 检查是否应该排除
                if self.should_exclude(&path) {
                    continue;
                }

                // 处理文件和目录
                if path.is_dir() {
                    // 添加到待扫描目录列表
                    directories_to_scan.push(path);
                } else if path.is_file() {
                    // 检查文件类型
                    if let Some(skill_metadata) = self.analyze_file(&path).await {
                        skills_found.push(skill_metadata);
                    }
                }
            }
        }

        Ok(skills_found)
    }

    /// 分析文件是否是技能文件
    async fn analyze_file(&self, file_path: &PathBuf) -> Option<SkillMetadata> {
        let _file_name = file_path.file_name()?.to_string_lossy();

        // 检查文件扩展名
        let ext = file_path.extension()?.to_string_lossy();

        match ext.as_ref() {
            "rs" => self.analyze_rust_skill(file_path).await,
            "json" => self.analyze_json_skill(file_path).await,
            "toml" => self.analyze_toml_skill(file_path).await,
            "yaml" | "yml" => self.analyze_yaml_skill(file_path).await,
            _ => None,
        }
    }

    /// 分析Rust技能文件
    async fn analyze_rust_skill(&self, file_path: &PathBuf) -> Option<SkillMetadata> {
        // 读取文件内容
        let content = match std::fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => return None,
        };

        // 简单解析Rust文件，查找技能定义
        if content.contains("impl Skill") || content.contains("define_skill!") {
            let file_name = file_path.file_stem()?.to_string_lossy();

            Some(SkillMetadata {
                name: file_name.to_string(),
                description: format!("Rust技能: {}", file_name),
                version: None,
                author: None,
                category: super::types::SkillCategory::Code,
                tags: vec!["rust".to_string(), "code".to_string()],
                input_schema: None,
                output_schema: None,
                required_permissions: vec!["file_system".to_string()],
                is_builtin: false,
                file_path: Some(file_path.clone()),
                config: std::collections::HashMap::new(),
            })
        } else {
            None
        }
    }

    /// 分析JSON技能文件
    async fn analyze_json_skill(&self, file_path: &PathBuf) -> Option<SkillMetadata> {
        // 读取和解析JSON文件
        let content = match std::fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => return None,
        };

        match serde_json::from_str::<SkillMetadata>(&content) {
            Ok(mut metadata) => {
                metadata.file_path = Some(file_path.clone());
                metadata.is_builtin = false;
                Some(metadata)
            }
            Err(_) => {
                // 尝试作为技能配置解析
                let file_name = file_path.file_stem()?.to_string_lossy();

                Some(SkillMetadata {
                    name: file_name.to_string(),
                    description: format!("JSON技能配置: {}", file_name),
                    version: None,
                    author: None,
                    category: super::types::SkillCategory::Other,
                    tags: vec!["json".to_string(), "config".to_string()],
                    input_schema: None,
                    output_schema: None,
                    required_permissions: Vec::new(),
                    is_builtin: false,
                    file_path: Some(file_path.clone()),
                    config: std::collections::HashMap::new(),
                })
            }
        }
    }

    /// 分析TOML技能文件
    async fn analyze_toml_skill(&self, _file_path: &PathBuf) -> Option<SkillMetadata> {
        // TOML技能解析（占位符）
        None
    }

    /// 分析YAML技能文件
    async fn analyze_yaml_skill(&self, _file_path: &PathBuf) -> Option<SkillMetadata> {
        // YAML技能解析（占位符）
        None
    }

    /// 清除扫描历史
    pub fn clear_scanned_paths(&mut self) {
        self.scanned_paths.clear();
    }

    /// 获取已扫描的路径数量
    pub fn scanned_paths_count(&self) -> usize {
        self.scanned_paths.len()
    }
}

impl Default for SkillDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_should_exclude() {
        let discovery = SkillDiscovery::new();

        // 测试排除模式
        let test_cases = vec![
            (PathBuf::from(".git"), true),
            (PathBuf::from("node_modules/test"), true),
            (PathBuf::from("target/debug"), true),
            (PathBuf::from("src/main.rs"), false),
            (PathBuf::from(".claude/skills"), false),
        ];

        for (path, should_exclude) in test_cases {
            assert_eq!(
                discovery.should_exclude(&path),
                should_exclude,
                "路径 {:?} 排除结果不正确",
                path
            );
        }
    }

    #[test]
    fn test_get_default_search_directories() {
        let discovery = SkillDiscovery::new();
        let dirs = discovery.get_default_search_directories();

        // 至少应该包含当前目录
        assert!(dirs.iter().any(|dir| dir == &PathBuf::from("./.claude/skills")));
        assert!(dirs.iter().any(|dir| dir == &PathBuf::from("./skills")));
    }

    #[tokio::test]
    async fn test_scan_directory_empty() {
        let temp_dir = tempdir().unwrap();
        let discovery = SkillDiscovery::new();
        let registry = SkillRegistry::new();

        let skills = discovery.scan_directory(&temp_dir.path().to_path_buf(), &registry).await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_scan_directory_exclude() {
        let temp_dir = tempdir().unwrap();

        // 创建应该被排除的目录
        let git_dir = temp_dir.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();

        let mut discovery = SkillDiscovery::new();
        let registry = SkillRegistry::new();

        let skills = discovery.scan_directory(&temp_dir.path().to_path_buf(), &registry).await.unwrap();
        assert!(skills.is_empty());
    }
}