# FS-rhei-callbacks: Transition Callback Examples

This document provides practical examples of state transition callbacks across all supported languages (TypeScript, Python, Java, and CLI/Bash). Each example demonstrates a specific use-case with the `TransitionContext` data structure.

## Table of Contents

1. [Basic Transition Approval](#1-basic-transition-approval)
2. [Dependency Validation](#2-dependency-validation)
3. [Data Passing Between Callbacks](#3-data-passing-between-callbacks)
4. [State Redirection](#4-state-redirection)
5. [Accessing Custom Metadata](#5-accessing-custom-metadata)
6. [Environment-Aware Logic](#6-environment-aware-logic)
7. [Publishing Complete Transition History](#7-publishing-complete-transition-history)

---

## 1. Basic Transition Approval

The simplest callback pattern: approve or reject a transition based on a condition.

### 1.1. TypeScript

```typescript
import { Rhei, TransitionContext, TransitionResult } from 'rhei-api';

const rhei = new Rhei({ rheiPath: './workflow.rhei.md' });

rhei.onLeave('draft', 'pending', async (ctx: TransitionContext): Promise<TransitionResult> => {
  // Reject if task title is empty
  if (!ctx.task.title.trim()) {
    return { success: false, error: 'Task title cannot be empty' };
  }
  return { success: true };
});
```

### 1.2. Python

```python
from rhei import Rhei, TransitionContext, TransitionResult

rhei = Rhei(rhei_path="./workflow.rhei.md")

@rhei.on_leave("draft", "pending")
def validate_title(ctx: TransitionContext) -> TransitionResult:
    """Reject if task title is empty."""
    if not ctx.task.title.strip():
        return TransitionResult(success=False, error="Task title cannot be empty")
    return TransitionResult(success=True)
```

### 1.3. Java

```java
import io.rhei.Rhei;
import io.rhei.TransitionContext;
import io.rhei.TransitionResult;

public class BasicCallbacks {
    public static TransitionResult validateTitle(TransitionContext ctx) {
        // Reject if task title is empty
        if (ctx.getTask().getTitle().trim().isEmpty()) {
            return TransitionResult.failure("Task title cannot be empty");
        }
        return TransitionResult.success();
    }
}
```

### 1.4. Bash (CLI)

```bash
#!/usr/bin/env bash
# handlers.sh

validate_title() {
    local context
    context=$(cat)  # Read JSON from stdin

    title=$(echo "$context" | jq -r '.task.title')

    if [[ -z "${title// /}" ]]; then
        echo '{"success": false, "error": "Task title cannot be empty"}'
    else
        echo '{"success": true}'
    fi
}
```

---

## 2. Dependency Validation

Check that all prior tasks are completed before allowing a transition.

### 2.1. TypeScript

```typescript
rhei.onLeave('pending', 'in-progress', async (ctx: TransitionContext): Promise<TransitionResult> => {
  const { task, rhei } = ctx;

  for (const depId of task.metadata.dependsOn) {
    const depTask = rhei.tasks.find(t => t.id === depId);

    if (!depTask) {
      return { success: false, error: `Dependency ${depId} not found` };
    }

    if (depTask.metadata.state !== 'completed') {
      return {
        success: false,
        error: `Dependency "${depTask.title}" is ${depTask.metadata.state}, not completed`
      };
    }
  }

  return { success: true };
});
```

### 2.2. Python

```python
@rhei.on_leave("pending", "in-progress")
def check_dependencies(ctx: TransitionContext) -> TransitionResult:
    """Ensure all dependencies are completed."""
    task = ctx.task
    rhei = ctx.rhei

    for dep_id in task.metadata.depends_on:
        dep_task = next((t for t in rhei.tasks if t.id == dep_id), None)

        if dep_task is None:
            return TransitionResult(
                success=False,
                error=f"Dependency {dep_id} not found"
            )

        if dep_task.metadata.state != "completed":
            return TransitionResult(
                success=False,
                error=f'Dependency "{dep_task.title}" is {dep_task.metadata.state}, not completed'
            )

    return TransitionResult(success=True)
```

### 2.3. Java

```java
public static TransitionResult checkDependencies(TransitionContext ctx) {
    Task task = ctx.getTask();
    Rhei rhei = ctx.getRhei();

    for (Object depId : task.getMetadata().getDependsOn()) {
        Task depTask = rhei.getTasks().stream()
            .filter(t -> t.getId().equals(depId))
            .findFirst()
            .orElse(null);

        if (depTask == null) {
            return TransitionResult.failure("Dependency " + depId + " not found");
        }

        String depState = depTask.getMetadata().getState();
        if (!"completed".equals(depState)) {
            return TransitionResult.failure(
                String.format("Dependency \"%s\" is %s, not completed",
                    depTask.getTitle(), depState)
            );
        }
    }

    return TransitionResult.success();
}
```

### 2.4. Bash (CLI)

```bash
check_dependencies() {
    local context
    context=$(cat)

    task_id=$(echo "$context" | jq -r '.task.id')
    dependencies=$(echo "$context" | jq -r '.task.metadata.dependsOn[]' 2>/dev/null)

    for dep_id in $dependencies; do
        dep_state=$(echo "$context" | jq -r \
            ".rhei.tasks[] | select(.id == \"$dep_id\" or .id == $dep_id) | .metadata.state")
        dep_title=$(echo "$context" | jq -r \
            ".rhei.tasks[] | select(.id == \"$dep_id\" or .id == $dep_id) | .title")

        if [[ -z "$dep_state" ]]; then
            echo "{\"success\": false, \"error\": \"Dependency $dep_id not found\"}"
            return 0
        fi

        if [[ "$dep_state" != "completed" ]]; then
            echo "{\"success\": false, \"error\": \"Dependency \\\"$dep_title\\\" is $dep_state, not completed\"}"
            return 0
        fi
    done

    echo '{"success": true}'
}
```

---

## 3. Data Passing Between Callbacks

Pass data from `on_leave` to `on_enter` via `transitionData`.

### 3.1. TypeScript

```typescript
// on_leave: collect data for the next phase
rhei.onLeave('processing', 'review', async (ctx: TransitionContext): Promise<TransitionResult> => {
  const results = await collectProcessingResults(ctx.task.id);

  return {
    success: true,
    data: {
      processedAt: ctx.transition.timestamp,
      artifactCount: results.artifacts.length,
      metrics: results.metrics
    }
  };
});

// on_enter: use data from on_leave
rhei.onEnter('review', async (ctx: TransitionContext): Promise<TransitionResult> => {
  // Access data passed from on_leave
  const { processedAt, artifactCount, metrics } = ctx.transitionData;

  console.log(`Reviewing ${artifactCount} artifacts processed at ${processedAt}`);
  console.log(`Quality score: ${metrics.qualityScore}`);

  // Notify reviewers with context
  await notifyReviewers({
    taskId: ctx.task.id,
    taskTitle: ctx.task.title,
    artifactCount,
    metrics
  });

  return { success: true };
});
```

### 3.2. Python

```python
@rhei.on_leave("processing", "review")
def package_results(ctx: TransitionContext) -> TransitionResult:
    """Collect processing results for review phase."""
    results = collect_processing_results(ctx.task.id)

    return TransitionResult(
        success=True,
        data={
            "processed_at": ctx.transition.timestamp,
            "artifact_count": len(results.artifacts),
            "metrics": results.metrics
        }
    )

@rhei.on_enter("review")
def setup_review(ctx: TransitionContext) -> TransitionResult:
    """Use data from on_leave to set up review."""
    processed_at = ctx.transition_data.get("processed_at")
    artifact_count = ctx.transition_data.get("artifact_count")
    metrics = ctx.transition_data.get("metrics", {})

    print(f"Reviewing {artifact_count} artifacts processed at {processed_at}")
    print(f"Quality score: {metrics.get('quality_score')}")

    notify_reviewers(
        task_id=ctx.task.id,
        task_title=ctx.task.title,
        artifact_count=artifact_count,
        metrics=metrics
    )

    return TransitionResult(success=True)
```

### 3.3. Java

```java
public static TransitionResult packageResults(TransitionContext ctx) {
    ProcessingResults results = collectProcessingResults(ctx.getTask().getId());

    Map<String, Object> data = new HashMap<>();
    data.put("processedAt", ctx.getTransition().getTimestamp());
    data.put("artifactCount", results.getArtifacts().size());
    data.put("metrics", results.getMetrics());

    return TransitionResult.success().withData(data);
}

public static TransitionResult setupReview(TransitionContext ctx) {
    Map<String, Object> transitionData = ctx.getTransitionData();

    String processedAt = (String) transitionData.get("processedAt");
    Integer artifactCount = (Integer) transitionData.get("artifactCount");
    Map<String, Object> metrics = (Map<String, Object>) transitionData.get("metrics");

    System.out.printf("Reviewing %d artifacts processed at %s%n",
        artifactCount, processedAt);
    System.out.printf("Quality score: %s%n", metrics.get("qualityScore"));

    notifyReviewers(ctx.getTask().getId(), ctx.getTask().getTitle(),
        artifactCount, metrics);

    return TransitionResult.success();
}
```

### 3.4. Bash (CLI)

```bash
package_results() {
    local context
    context=$(cat)

    task_id=$(echo "$context" | jq -r '.task.id')
    timestamp=$(echo "$context" | jq -r '.transition.timestamp')

    # Simulate collecting results
    artifact_count=42
    quality_score=0.95

    cat <<EOF
{
    "success": true,
    "data": {
        "processedAt": "$timestamp",
        "artifactCount": $artifact_count,
        "metrics": {"qualityScore": $quality_score}
    }
}
EOF
}

setup_review() {
    local context
    context=$(cat)

    # Access data passed from on_leave via transitionData
    processed_at=$(echo "$context" | jq -r '.transitionData.processedAt')
    artifact_count=$(echo "$context" | jq -r '.transitionData.artifactCount')
    quality_score=$(echo "$context" | jq -r '.transitionData.metrics.qualityScore')

    echo "Reviewing $artifact_count artifacts processed at $processed_at" >&2
    echo "Quality score: $quality_score" >&2

    echo '{"success": true}'
}
```

---

## 4. State Redirection

Dynamically redirect to a different state based on runtime conditions.

### 4.1. TypeScript

```typescript
rhei.onLeave('validation', 'approved', async (ctx: TransitionContext): Promise<TransitionResult> => {
  const validationResult = await runValidation(ctx.task);

  if (validationResult.hasBlockingErrors) {
    // Redirect to rejected instead of approved
    return {
      success: true,
      nextState: 'rejected',
      data: { errors: validationResult.errors }
    };
  }

  if (validationResult.hasWarnings) {
    // Redirect to manual review
    return {
      success: true,
      nextState: 'manual-review',
      data: { warnings: validationResult.warnings }
    };
  }

  // Proceed to approved as originally requested
  return { success: true };
});
```

### 4.2. Python

```python
@rhei.on_leave("validation", "approved")
def validate_and_route(ctx: TransitionContext) -> TransitionResult:
    """Validate and potentially redirect to different states."""
    validation_result = run_validation(ctx.task)

    if validation_result.has_blocking_errors:
        # Redirect to rejected instead of approved
        return TransitionResult(
            success=True,
            next_state="rejected",
            data={"errors": validation_result.errors}
        )

    if validation_result.has_warnings:
        # Redirect to manual review
        return TransitionResult(
            success=True,
            next_state="manual-review",
            data={"warnings": validation_result.warnings}
        )

    # Proceed to approved as originally requested
    return TransitionResult(success=True)
```

### 4.3. Java

```java
public static TransitionResult validateAndRoute(TransitionContext ctx) {
    ValidationResult validation = runValidation(ctx.getTask());

    if (validation.hasBlockingErrors()) {
        // Redirect to rejected instead of approved
        return TransitionResult.builder()
            .success(true)
            .nextState("rejected")
            .data("errors", validation.getErrors())
            .build();
    }

    if (validation.hasWarnings()) {
        // Redirect to manual review
        return TransitionResult.builder()
            .success(true)
            .nextState("manual-review")
            .data("warnings", validation.getWarnings())
            .build();
    }

    // Proceed to approved as originally requested
    return TransitionResult.success();
}
```

### 4.4. Bash (CLI)

```bash
validate_and_route() {
    local context
    context=$(cat)

    task_id=$(echo "$context" | jq -r '.task.id')

    # Simulate validation (in practice, run actual validation)
    has_blocking_errors=false
    has_warnings=true

    if [[ "$has_blocking_errors" == "true" ]]; then
        cat <<EOF
{
    "success": true,
    "nextState": "rejected",
    "data": {"errors": ["Critical validation failure"]}
}
EOF
    elif [[ "$has_warnings" == "true" ]]; then
        cat <<EOF
{
    "success": true,
    "nextState": "manual-review",
    "data": {"warnings": ["Non-critical issue detected"]}
}
EOF
    else
        echo '{"success": true}'
    fi
}
```

---

## 5. Accessing Custom Metadata

Use arbitrary metadata fields stored in the plan file's YAML frontmatter.

### 5.1. TypeScript

```typescript
rhei.onLeave('queued', 'processing', async (ctx: TransitionContext): Promise<TransitionResult> => {
  const { task } = ctx;

  // Access custom metadata fields
  const priority = task.metadata.priority ?? 'normal';
  const maxRetries = task.metadata.maxRetries ?? 3;
  const assignee = task.metadata.assignee;
  const estimatedDuration = task.metadata.estimatedDuration;

  console.log(`Processing ${priority}-priority task for ${assignee}`);
  console.log(`Estimated duration: ${estimatedDuration}`);

  // Enforce priority-based rules
  if (priority === 'critical' && !assignee) {
    return {
      success: false,
      error: 'Critical tasks must have an assignee'
    };
  }

  return {
    success: true,
    data: { maxRetries, startedBy: ctx.environment.platform }
  };
});
```

### 5.2. Python

```python
@rhei.on_leave("queued", "processing")
def check_metadata(ctx: TransitionContext) -> TransitionResult:
    """Access and validate custom metadata fields."""
    task = ctx.task

    # Access custom metadata fields
    priority = task.metadata.get("priority", "normal")
    max_retries = task.metadata.get("maxRetries", 3)
    assignee = task.metadata.get("assignee")
    estimated_duration = task.metadata.get("estimatedDuration")

    print(f"Processing {priority}-priority task for {assignee}")
    print(f"Estimated duration: {estimated_duration}")

    # Enforce priority-based rules
    if priority == "critical" and not assignee:
        return TransitionResult(
            success=False,
            error="Critical tasks must have an assignee"
        )

    return TransitionResult(
        success=True,
        data={"maxRetries": max_retries, "startedBy": ctx.environment.platform}
    )
```

### 5.3. Java

```java
public static TransitionResult checkMetadata(TransitionContext ctx) {
    TaskMetadata metadata = ctx.getTask().getMetadata();

    // Access custom metadata fields
    String priority = metadata.getString("priority", "normal");
    int maxRetries = metadata.getInt("maxRetries", 3);
    String assignee = metadata.getString("assignee");
    String estimatedDuration = metadata.getString("estimatedDuration");

    System.out.printf("Processing %s-priority task for %s%n", priority, assignee);
    System.out.printf("Estimated duration: %s%n", estimatedDuration);

    // Enforce priority-based rules
    if ("critical".equals(priority) && assignee == null) {
        return TransitionResult.failure("Critical tasks must have an assignee");
    }

    return TransitionResult.success()
        .withData("maxRetries", maxRetries)
        .withData("startedBy", ctx.getEnvironment().getPlatform());
}
```

### 5.4. Bash (CLI)

```bash
check_metadata() {
    local context
    context=$(cat)

    # Access custom metadata fields
    priority=$(echo "$context" | jq -r '.task.metadata.priority // "normal"')
    max_retries=$(echo "$context" | jq -r '.task.metadata.maxRetries // 3')
    assignee=$(echo "$context" | jq -r '.task.metadata.assignee // empty')
    estimated_duration=$(echo "$context" | jq -r '.task.metadata.estimatedDuration // "unknown"')
    platform=$(echo "$context" | jq -r '.environment.platform')

    echo "Processing $priority-priority task for $assignee" >&2
    echo "Estimated duration: $estimated_duration" >&2

    # Enforce priority-based rules
    if [[ "$priority" == "critical" && -z "$assignee" ]]; then
        echo '{"success": false, "error": "Critical tasks must have an assignee"}'
        return 0
    fi

    cat <<EOF
{
    "success": true,
    "data": {
        "maxRetries": $max_retries,
        "startedBy": "$platform"
    }
}
EOF
}
```

---

## 6. Environment-Aware Logic

Adapt behavior based on the execution environment.

### 6.1. TypeScript

```typescript
rhei.onEnter('deploying', async (ctx: TransitionContext): Promise<TransitionResult> => {
  const { environment, task, rhei } = ctx;

  console.log(`Deploying from ${environment.platform} v${environment.version}`);
  console.log(`Working directory: ${environment.workingDirectory}`);

  // Platform-specific deployment logic
  switch (environment.platform) {
    case 'cli':
      // CLI runs actual shell commands
      await execShell(`deploy.sh --task ${task.id}`);
      break;

    case 'nodejs':
      // Node.js uses programmatic deployment
      await deployService.deploy(task.id, {
        rheiPath: rhei.path,
        dryRun: false
      });
      break;

    case 'python':
    case 'java':
      // Other platforms delegate to CLI
      await execShell(`rhei-cli deploy --task ${task.id} --input ${rhei.path}`);
      break;
  }

  return {
    success: true,
    data: { deployedFrom: environment.platform }
  };
});
```

### 6.2. Python

```python
import subprocess

@rhei.on_enter("deploying")
def deploy_task(ctx: TransitionContext) -> TransitionResult:
    """Deploy with platform-aware logic."""
    env = ctx.environment
    task = ctx.task
    rhei = ctx.rhei

    print(f"Deploying from {env.platform} v{env.version}")
    print(f"Working directory: {env.working_directory}")

    # Platform-specific deployment logic
    if env.platform == "cli":
        # CLI runs actual shell commands
        subprocess.run(["deploy.sh", "--task", str(task.id)], check=True)

    elif env.platform == "python":
        # Python uses native deployment library
        deploy_service.deploy(task.id, rhei_path=rhei.path, dry_run=False)

    else:
        # Other platforms delegate to CLI
        subprocess.run([
            "rhei-cli", "deploy",
            "--task", str(task.id),
            "--input", rhei.path
        ], check=True)

    return TransitionResult(
        success=True,
        data={"deployedFrom": env.platform}
    )
```

### 6.3. Java

```java
public static TransitionResult deployTask(TransitionContext ctx) {
    Environment env = ctx.getEnvironment();
    Task task = ctx.getTask();
    Rhei rhei = ctx.getRhei();

    System.out.printf("Deploying from %s v%s%n", env.getPlatform(), env.getVersion());
    System.out.printf("Working directory: %s%n", env.getWorkingDirectory());

    try {
        // Platform-specific deployment logic
        switch (env.getPlatform()) {
            case "java":
                // Java uses native deployment service
                DeployService.deploy(task.getId(), rhei.getPath());
                break;

            case "cli":
                // CLI runs shell commands
                Runtime.getRuntime().exec(new String[]{
                    "deploy.sh", "--task", task.getId().toString()
                });
                break;

            default:
                // Other platforms delegate to CLI
                Runtime.getRuntime().exec(new String[]{
                    "rhei-cli", "deploy",
                    "--task", task.getId().toString(),
                    "--input", rhei.getPath()
                });
        }

        return TransitionResult.success()
            .withData("deployedFrom", env.getPlatform());

    } catch (Exception e) {
        return TransitionResult.failure("Deployment failed: " + e.getMessage());
    }
}
```

### 6.4. Bash (CLI)

```bash
deploy_task() {
    local context
    context=$(cat)

    platform=$(echo "$context" | jq -r '.environment.platform')
    version=$(echo "$context" | jq -r '.environment.version')
    working_dir=$(echo "$context" | jq -r '.environment.workingDirectory')
    task_id=$(echo "$context" | jq -r '.task.id')
    rhei_path=$(echo "$context" | jq -r '.rhei.path')

    echo "Deploying from $platform v$version" >&2
    echo "Working directory: $working_dir" >&2

    # CLI can run deployment directly
    if ./deploy.sh --task "$task_id" --input "$rhei_path"; then
        cat <<EOF
{
    "success": true,
    "data": {"deployedFrom": "$platform"}
}
EOF
    else
        cat <<EOF
{
    "success": false,
    "error": "Deployment failed with exit code $?"
}
EOF
    fi
}
```

---

## 7. Publishing Complete Transition History

An `on_enter` publisher can combine the committed central-ledger prefix with
the pending firing in its context. It must not wait for or guess this firing's
ledger row: the row is appended only after the callback succeeds
([§FS-rhei-transitions.1.2](rhei-transitions.spec.md#12-firing-identity-and-callback-time-visibility)).

```typescript
const committed = await readLines('runtime/state-transitions.log');
const current = `${ctx.task.id} ${ctx.transition.from}@${ctx.transition.to}`;
const completeHistory =
  ctx.transition.ledgerStatus === 'pending' ? [...committed, current] : committed;
await publish({ history: completeHistory, firingId: ctx.transition.firingId });
```

The equivalent Python properties are `firing_id` and `ledger_status`; Java
uses `getFiringId()` and `getLedgerStatus()`. A CLI callback reads the canonical
JSON names above or the equal environment values
`RHEI_TRANSITION_FIRING_ID` and `RHEI_TRANSITION_LEDGER_STATUS`.

A consumer may remember `firingId` to suppress a replay of captured input.
That is a local policy, not authentication or an exactly-once guarantee. A
direct operator invocation of the callback receives no fresh Rhei-issued ID,
and a later real transition—even an identical edge or self-loop—receives a new
one. Decoders must allow unknown JSON fields so additive callback context
changes remain compatible.

---

## Related Documentation

- [Transitions Specification](rhei-transitions.spec.md) - TransitionContext, TransitionResult, YAML schema, and execution semantics
- [States Specification](rhei-states.spec.md) - State machine format and default states
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Specification](rhei-plan-language.spec.md) - Formal grammar and semantic constraints
