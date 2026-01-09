# Verification-Driven Development Hierarchy
## Highlighter Neovim Plugin

**Last Updated:** 2026-01-05
**VDD Round:** 0 (Initial Decomposition)

---

## Epic 1: Core Functionality Robustness
**Goal:** Ensure highlighting logic handles all edge cases without corruption or data loss

### Issue 1.1: Selection Boundary Handling
**Acceptance Criteria:** All selection modes and boundaries work correctly

#### Sub-issue 1.1.1: Character-wise visual selection
- [ ] Single character selection
- [ ] Multi-character same-line selection
- [ ] Selection at line boundaries (col 0, end of line)
- [ ] Empty lines selection
- [ ] Whitespace-only selection

#### Sub-issue 1.1.2: Line-wise visual selection
- [ ] Single line selection
- [ ] Multi-line selection
- [ ] Entire buffer selection
- [ ] Selection with mixed content (code, whitespace, special chars)

#### Sub-issue 1.1.3: Block-wise visual selection
- [ ] Rectangular block selection
- [ ] Block selection across lines of different lengths
- [ ] Block selection with tabs vs spaces

### Issue 1.2: Extmark Priority Management
**Acceptance Criteria:** Overlapping highlights render correctly with proper layering

#### Sub-issue 1.2.1: Priority calculation logic
- [ ] Verify priority increment logic (currently max_priority + 1)
- [ ] Handle case where no existing extmarks exist
- [ ] Handle case where many overlapping extmarks exist (>100)
- [ ] Verify priority overflow handling (u32::MAX edge case)

#### Sub-issue 1.2.2: Overlapping highlight scenarios
- [ ] Same region, different colors applied sequentially
- [ ] Partially overlapping regions
- [ ] Nested highlights (small selection within larger)
- [ ] Complete overlap (identical start/end positions)

### Issue 1.3: Namespace and Extmark Management
**Acceptance Criteria:** Extmarks are correctly created, queried, and cleared

#### Sub-issue 1.3.1: Namespace isolation
- [ ] Verify plugin namespace doesn't conflict with other plugins
- [ ] Test namespace creation at module initialization
- [ ] Verify lazy_static initialization is thread-safe

#### Sub-issue 1.3.2: Clear operations
- [ ] HighlighterClear removes all highlights correctly
- [ ] Partial line clear works as expected (clear_line function)
- [ ] Clearing doesn't affect other plugins' extmarks
- [ ] Clear operation on empty buffer doesn't error

---

## Epic 2: Testing Infrastructure
**Goal:** Achieve >80% test coverage for critical paths with automated verification

### Issue 2.1: Unit Test Framework
**Acceptance Criteria:** Core logic functions have comprehensive unit tests

#### Sub-issue 2.1.1: Utility functions
- [ ] Test `entire_line()` with various column positions
- [ ] Test `zero_based_row()` conversion logic
- [ ] Test `end_of_line()` for various line lengths
- [ ] Test with UTF-8 multi-byte characters

#### Sub-issue 2.1.2: Priority calculation
- [ ] Test `highest_line_priority()` with mock extmarks
- [ ] Test entire_line path triggering clear_line
- [ ] Test priority calculation with no existing highlights
- [ ] Test priority calculation with maximum priority extmarks

### Issue 2.2: Integration Tests
**Acceptance Criteria:** End-to-end workflows tested in Neovim environment

#### Sub-issue 2.2.1: Command execution
- [ ] Test :Highlighter command with visual selection
- [ ] Test :HighlighterClear command
- [ ] Test commands in various buffer states (empty, large, special chars)

#### Sub-issue 2.2.2: Keymap functionality
- [ ] Test <C-h> in visual mode
- [ ] Test <leader>ch in normal mode
- [ ] Test keymaps don't interfere with other plugins

### Issue 2.3: Property-Based Testing
**Acceptance Criteria:** Fuzzing reveals no crashes or undefined behavior

#### Sub-issue 2.3.1: Input fuzzing
- [ ] Fuzz selection boundaries (random row/col combinations)
- [ ] Fuzz buffer content (random UTF-8, special chars, control chars)
- [ ] Fuzz extmark priorities (edge values, MAX, 0)

---

## Epic 3: Error Handling & Recovery
**Goal:** Graceful degradation with clear error messages, no data corruption

### Issue 3.1: API Error Handling
**Acceptance Criteria:** All Neovim API calls handle errors gracefully

#### Sub-issue 3.1.1: Buffer operations
- [ ] Handle buffer invalidation (buffer closed during operation)
- [ ] Handle readonly buffers
- [ ] Handle buffers with 'modifiable' set to false
- [ ] Handle invalid line ranges

#### Sub-issue 3.1.2: Mark operations
- [ ] Handle missing visual marks (< and >)
- [ ] Handle corrupted mark data
- [ ] Handle marks in invalid positions

#### Sub-issue 3.1.3: UI operations
- [ ] Handle vim.ui.select cancellation
- [ ] Handle vim.ui.select when no selection made
- [ ] Handle Lua runtime errors in callbacks

### Issue 3.2: User Feedback
**Acceptance Criteria:** Users understand what went wrong and how to fix it

#### Sub-issue 3.2.1: Error messages
- [ ] Replace .unwrap() with proper error handling
- [ ] Provide contextual error messages
- [ ] Use oxi::print! or nvim_err_writeln for user-visible errors

#### Sub-issue 3.2.2: Logging
- [ ] Add debug logging for extmark operations
- [ ] Add trace logging for priority calculations
- [ ] Configurable log level

---

## Epic 4: Performance Optimization
**Goal:** Handle large files and selections without noticeable lag

### Issue 4.1: Algorithmic Efficiency
**Acceptance Criteria:** Operations complete in <100ms for typical use cases

#### Sub-issue 4.1.1: Extmark query optimization
- [ ] Benchmark get_extmarks for large highlight counts
- [ ] Optimize priority calculation (avoid redundant queries)
- [ ] Consider caching priority information

#### Sub-issue 4.1.2: Bulk operations
- [ ] Optimize multi-line highlighting (avoid N API calls)
- [ ] Batch extmark creation where possible
- [ ] Optimize clear operations for large ranges

### Issue 4.2: Memory Management
**Acceptance Criteria:** No memory leaks, bounded memory usage

#### Sub-issue 4.2.1: Resource cleanup
- [ ] Verify extmarks are properly cleaned up
- [ ] Verify no Lua reference cycles
- [ ] Profile memory usage over extended sessions

---

## Epic 5: Documentation & Usability
**Goal:** Clear documentation enabling users to install and use without friction

### Issue 5.1: User Documentation
**Acceptance Criteria:** README with installation, usage, and examples

#### Sub-issue 5.1.1: README.md
- [ ] Project description and purpose
- [ ] Installation instructions (build from source)
- [ ] Usage examples with screenshots/GIFs
- [ ] Keybinding reference
- [ ] FAQ and troubleshooting

#### Sub-issue 5.1.2: Neovim help docs
- [ ] Create doc/highlighter.txt
- [ ] Document commands and keymaps
- [ ] Document available colors
- [ ] Document configuration options

### Issue 5.2: Code Documentation
**Acceptance Criteria:** All public APIs documented with rustdoc

#### Sub-issue 5.2.1: Function documentation
- [ ] Document public functions with examples
- [ ] Document edge cases and assumptions
- [ ] Document error conditions

#### Sub-issue 5.2.2: Architecture documentation
- [ ] Create ARCHITECTURE.md explaining design decisions
- [ ] Document extmark strategy
- [ ] Document priority system

---

## Epic 6: Configuration & Extensibility
**Goal:** Users can customize colors, keybindings, and behavior

### Issue 6.1: Color Customization
**Acceptance Criteria:** Users can define custom colors

#### Sub-issue 6.1.1: Configuration API
- [ ] Design Lua API for user configuration
- [ ] Allow users to add/remove colors
- [ ] Validate color hex codes

#### Sub-issue 6.1.2: Color persistence
- [ ] Consider saving highlights to file
- [ ] Consider loading highlights from file
- [ ] Handle version compatibility

### Issue 6.2: Behavior Customization
**Acceptance Criteria:** Core behaviors are configurable

#### Sub-issue 6.2.1: Keybinding configuration
- [ ] Allow users to disable default keybindings
- [ ] Provide setup function for custom keybindings

#### Sub-issue 6.2.2: Priority configuration
- [ ] Allow base priority configuration
- [ ] Allow priority increment configuration

---

## Epic 7: Build & Distribution
**Goal:** Reproducible builds across platforms with easy installation

### Issue 7.1: Build System
**Acceptance Criteria:** Build works on Linux, macOS, Windows

#### Sub-issue 7.1.1: Cross-platform build script
- [ ] Test build on Linux
- [ ] Test build on macOS
- [ ] Test build on Windows
- [ ] Add build dependencies documentation

#### Sub-issue 7.1.2: CI/CD pipeline
- [ ] GitHub Actions for automated builds
- [ ] Automated testing on commit
- [ ] Release automation

### Issue 7.2: Distribution
**Acceptance Criteria:** Users can install via package managers

#### Sub-issue 7.2.1: Plugin manager support
- [ ] Test with lazy.nvim
- [ ] Test with packer.nvim
- [ ] Test with vim-plug
- [ ] Document installation for each manager

---

## VDD Status Tracking

### Round 0: Initial State Analysis
- **Code Review Status:** Complete
- **Test Coverage:** 0% (no tests exist)
- **Known Issues:** Extensive use of .unwrap(), no error handling
- **Technical Debt:** Priority overflow handling, no documentation

### Round 1: Self-Review and Critical Fixes ✅
- **Date:** 2026-01-05
- **Adversary Review:** Self-identified critical issues addressed
- **Test Coverage:** ~35% (9 unit tests + 7 integration tests = 16 total tests)
- **Critiques Addressed:**
  1. ✅ Fixed namespace bug in clear() (was using 0, now uses *PLUGIN)
  2. ✅ Replaced all 18+ .unwrap() calls with proper error handling
  3. ✅ Added priority overflow protection (MAX_SAFE_PRIORITY constant)
  4. ✅ Replaced magic number 200 with BASE_PRIORITY constant
  5. ✅ Added color choice validation in perform_highlight
  6. ✅ Fixed off-by-one error in clear_line (row..=row+1 → row..=row)
  7. ✅ Documented UTF-8 byte length behavior in end_of_line
  8. ✅ Fixed typo "LIne" → "Line" in debug message
  9. ✅ Properly handle set_extmark return value (removed let _ =)
  10. ✅ Added comprehensive error messages for all failure modes
- **Code Quality Improvements:**
  - All functions now return Result types with descriptive errors
  - Added constants section with documented constants
  - Added section headers for better code organization
  - Improved function documentation with /// comments
  - Error messages include context (row, col, operation)
  - Added 4 new integration tests for edge cases

### Round 2: [Ready for External Adversarial Review]
- **Status:** Awaiting fresh adversarial session
- **Next Steps:** Run ADVERSARIAL_REVIEW_PACKAGE.md in fresh Claude session
- **Expected Focus:** Architecture, remaining edge cases, performance

---

## Next Steps for VDD Cycle
1. ✅ Create this hierarchy document
2. ⏳ Implement automated testing infrastructure (Epic 2)
3. ⏳ Conduct HITL verification of existing functionality
4. ⏳ Prepare codebase for adversarial review
5. ⏳ Run first adversarial review session
6. ⏳ Address critiques and iterate
