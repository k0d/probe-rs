use super::{debug_info, extract_file, unit_info::UnitInfo, ColumnType, SourceLocation};

pub(crate) type FunctionDieType<'abbrev, 'unit> =
    gimli::DebuggingInformationEntry<'abbrev, 'unit, debug_info::GimliReader, usize>;

/// Reference to a DIE for a function
pub(crate) struct FunctionDie<'abbrev, 'unit, 'unit_info, 'debug_info> {
    pub(crate) unit_info: &'unit_info UnitInfo<'debug_info>,

    pub(crate) function_die: FunctionDieType<'abbrev, 'unit>,

    /// Only present for inlined functions, where this is a reference
    /// to the declaration of the function.
    pub(crate) abstract_die: Option<FunctionDieType<'abbrev, 'unit>>,
    /// The address of the first instruction in this function.
    pub(crate) low_pc: u64,
    /// The address of the first instruction after this funciton.
    pub(crate) high_pc: u64,
}

impl<'debugunit, 'abbrev, 'unit: 'debugunit, 'unit_info, 'debug_info>
    FunctionDie<'abbrev, 'unit, 'unit_info, 'debug_info>
{
    pub(crate) fn new(
        die: FunctionDieType<'abbrev, 'unit>,
        unit_info: &'unit_info UnitInfo<'debug_info>,
    ) -> Option<Self> {
        let tag = die.tag();

        match tag {
            gimli::DW_TAG_subprogram => Some(Self {
                unit_info,
                function_die: die,
                abstract_die: None,
                low_pc: 0,
                high_pc: 0,
            }),
            other_tag => {
                log::error!("FunctionDie has to has to have Tag DW_TAG_subprogram, but tag is {:?}. This is a bug, please report it.", other_tag.static_string());
                None
            }
        }
    }

    pub(crate) fn new_inlined(
        concrete_die: FunctionDieType<'abbrev, 'unit>,
        abstract_die: FunctionDieType<'abbrev, 'unit>,
        unit_info: &'unit_info UnitInfo<'debug_info>,
    ) -> Option<Self> {
        let tag = concrete_die.tag();

        match tag {
            gimli::DW_TAG_inlined_subroutine => Some(Self {
                unit_info,
                function_die: concrete_die,
                abstract_die: Some(abstract_die),
                low_pc: 0,
                high_pc: 0,
            }),
            other_tag => {
                log::error!("FunctionDie has to has to have Tag DW_TAG_inlined_subroutine, but tag is {:?}. This is a bug, please report it.", other_tag.static_string());
                None
            }
        }
    }

    pub(crate) fn is_inline(&self) -> bool {
        self.abstract_die.is_some()
    }

    pub(crate) fn function_name(&self) -> Option<String> {
        if let Some(fn_name_attr) = self.get_attribute(gimli::DW_AT_name) {
            match fn_name_attr.value() {
                gimli::AttributeValue::DebugStrRef(fn_name_ref) => {
                    match self.unit_info.debug_info.dwarf.string(fn_name_ref) {
                        Ok(fn_name_raw) => Some(String::from_utf8_lossy(&fn_name_raw).to_string()),
                        Err(error) => {
                            log::debug!("No value for DW_AT_name: {:?}: error", error);

                            None
                        }
                    }
                }
                value => {
                    log::debug!("Unexpected attribute value for DW_AT_name: {:?}", value);
                    None
                }
            }
        } else {
            log::debug!("DW_AT_name attribute not found, unable to retrieve function name");
            None
        }
    }

    /// Get the call site of an inlined function.
    ///
    /// If this function is not inlined (`is_inline()` returns false),
    /// this function returns `None`.
    pub(crate) fn inline_call_location(&self) -> Option<SourceLocation> {
        if !self.is_inline() {
            return None;
        }

        let file_name_attr = self.get_attribute(gimli::DW_AT_call_file)?;

        let (directory, file) = extract_file(
            self.unit_info.debug_info,
            &self.unit_info.unit,
            file_name_attr.value(),
        )?;
        let line = self
            .get_attribute(gimli::DW_AT_call_line)
            .and_then(|line| line.udata_value());

        let column =
            self.get_attribute(gimli::DW_AT_call_column)
                .map(|column| match column.udata_value() {
                    None => ColumnType::LeftEdge,
                    Some(c) => ColumnType::Column(c),
                });
        Some(SourceLocation {
            line,
            column,
            file: Some(file),
            directory: Some(directory),
            low_pc: Some(self.low_pc as u32),
            high_pc: Some(self.high_pc as u32),
        })
    }

    /// Resolve an attribute by looking through both the origin or abstract die entries.
    pub(crate) fn get_attribute(
        &self,
        attribute_name: gimli::DwAt,
    ) -> Option<debug_info::GimliAttribute> {
        let attribute = self
            .function_die
            .attr(attribute_name)
            .map_or(None, |attribute| attribute);

        // For inlined function, the *abstract instance* has to be checked if we cannot find the
        // attribute on the *concrete instance*.
        if self.is_inline() && attribute.is_none() {
            if let Some(origin) = self.abstract_die.as_ref() {
                origin
                    .attr(attribute_name)
                    .map_or(None, |attribute| attribute)
            } else {
                None
            }
        } else {
            attribute
        }
    }
}
