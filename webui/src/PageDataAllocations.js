import _ from "lodash";
import React from "react";
import ReactTable from "react-table";
import Graph from "./Graph.js";
import { FormGroup, Label, Input, Button, ButtonGroup, Modal, ModalFooter, ModalBody, ModalHeader, Badge } from "reactstrap";
import { Link } from "react-router-dom";
import { ContextMenu, MenuItem, ContextMenuTrigger } from "react-contextmenu";
import classNames from "classnames";
import Feather from "./Feather.js";
import Tabbed from "./Tabbed.js";
import { fmt_size, fmt_date_unix, fmt_date_timeval, fmt_hex16, fmt_uptime, fmt_uptime_timeval, update_query, create_query, extract_query, format_frame } from "./utils.js";

export class BacktraceGraph extends Graph {

    componentDidMount() {
        super.componentDidMount();
        fetch( (this.props.sourceUrl || "") + "/data/" + this.props.id + "/timeline?backtraces=" + this.props.backtrace_id )
            .then( rsp => rsp.json() )
            .then( json => {
                //this.setState( {data: json} )
                this.props.data = json;
                //this.cache = None;
                //this.forceRefresh();
                this.forceRefresh();
                this.forceUpdate();
            });
    }

}

const PERCENTAGE_REGEX = /^(\d+)%$/;
const DATE_REGEX = /^(\d{4})-(\d{1,2})-(\d{1,2})\s+(\d{1,2}):(\d{1,2}):(\d{1,2})$/;
const SIZE_REGEX = /^(\d+)(k|m|g|t)?$/;
const DURATION_REGEX = /^(\d+d)?(\d+h)?(\d+m)?(\d+s)?(\d+ms)?(\d+us)?|\d+$/;
const POSITIVE_INTEGER_REGEX = /^(\d+)$/;

function validate_percentage( value ) {
    return value.match( PERCENTAGE_REGEX );
}

function validate_date( value ) {
    return value.match( DATE_REGEX );
}

function validate_or_percentage( alt ) {
    return (value) => {
        return validate_percentage( value ) || alt( value );
    };
}

function validate_positive_integer( value ) {
    return value.match( POSITIVE_INTEGER_REGEX );
}

function format_or_percentage( alt ) {
    return (value) => {
        if( validate_percentage( value ) ) {
            return value;
        } else {
            return alt( value );
        }
    };
}

function parse_or_percentage( alt ) {
    return (value) => {
        if( validate_percentage( value ) ) {
            return value;
        } else {
            return alt( value );
        }
    };
}

function date_to_unix( value ) {
    const match = value.match( DATE_REGEX );
    const year = parseInt( match[ 1 ], 10 );
    const month = parseInt( match[ 2 ], 10 );
    const day = parseInt( match[ 3 ], 10 );
    const hour = parseInt( match[ 4 ], 10 );
    const minute = parseInt( match[ 5 ], 10 );
    const second = parseInt( match[ 6 ], 10 );
    return Math.floor( Date.UTC( year, month - 1, day, hour, minute, second ) / 1000 );
}

function validate_size( value ) {
    value = value.replace( " ", "" ).toLowerCase();
    return value.match( SIZE_REGEX );
}

function fmt_size_full( value ) {
    return value;
}

function parse_size( value ) {
    value = value.replace( " ", "" ).toLowerCase();
    const match = value.match( SIZE_REGEX );
    const unit = match[ 2 ];

    let mul = 1;
    if( unit === "k" ) {
        mul = 1000;
    } else if( unit === "m" ) {
        mul = 1000 * 1000;
    } else if( unit === "g" ) {
        mul = 1000 * 1000 * 1000;
    } else if( unit === "t" ) {
        mul = 1000 * 1000 * 1000 * 1000;
    }

    return parseInt( match[ 1 ], 10 ) * mul;
}

function validate_duration( value ) {
    value = value.replace( " ", "" ).toLowerCase();
    return value.length > 0 && value.match( DURATION_REGEX );
}

function fmt_duration_for_display( value ) {
    value = value.replace( " ", "" ).toLowerCase();
    if( value.match( POSITIVE_INTEGER_REGEX ) ) {
        value = value + "s";
    }

    return value;
}

function parse_integer( value ) {
    return parseInt( value, 10 );
}

function identity( value ) {
    return value + "";
}

const DATE_FIELD = {
    kind: "entry",
    validate: validate_date,
    format: fmt_date_unix,
    parse: date_to_unix
};

const DATE_OR_PERCENTAGE_FIELD = {
    kind: "entry",
    validate: validate_or_percentage( DATE_FIELD.validate ),
    format: format_or_percentage( DATE_FIELD.format ),
    parse: parse_or_percentage( DATE_FIELD.parse )
};

const SIZE_FIELD = {
    kind: "entry",
    validate: validate_size,
    format: fmt_size_full,
    parse: parse_size
};

const POSITIVE_INTEGER_FIELD = {
    kind: "entry",
    validate: validate_positive_integer,
    format: identity,
    parse: parse_integer
};

const POSITIVE_INTEGER_OR_PERCENTAGE_FIELD = {
    kind: "entry",
    validate: validate_or_percentage( POSITIVE_INTEGER_FIELD.validate ),
    format: format_or_percentage( POSITIVE_INTEGER_FIELD.format ),
    parse: parse_or_percentage( POSITIVE_INTEGER_FIELD.parse )
};

const DURATION_FIELD = {
    kind: "entry",
    validate: validate_duration,
    format: identity,
    parse: identity
};

const DURATION_OR_PERCENTAGE_FIELD = {
    kind: "entry",
    validate: validate_or_percentage( DURATION_FIELD.validate ),
    format: format_or_percentage( DURATION_FIELD.format ),
    parse: parse_or_percentage( DURATION_FIELD.parse )
}

const RADIO_FIELD = {
    kind: "radio",
    validate: function( value ) { return value !== ""; },
    format: function( value ) { return value; },
    parse: function( value ) { return value; }
};

const REGEX_FIELD = {
    kind: "entry",
    validate: function( value ) {
        if( value === "" ) {
            return false;
        }

        try {
            new RegExp( value );
        } catch( exception ) {
            return false;
        }

        return true;
    },
    format: identity,
    parse: identity
};

function fmt_or_percent( formatter ) {
    return function( value ) {
        if( value.endsWith( "%" ) ) {
            return value;
        } else {
            return formatter( value );
        }
    };
}

const FIELDS = {
    from: {
        ...DATE_OR_PERCENTAGE_FIELD,
        label: "From",
        badge: value => "From " + (fmt_date_unix( value ) || value)
    },
    to: {
        ...DATE_OR_PERCENTAGE_FIELD,
        label: "To",
        badge: value => "Until " + (fmt_date_unix( value ) || value)
    },
    lifetime_min: {
        ...DURATION_FIELD,
        label: "From",
        badge: value => "Living at least " + fmt_duration_for_display( value )
    },
    lifetime_max: {
        ...DURATION_FIELD,
        label: "To",
        badge: value => "Living at most " + fmt_duration_for_display( value )
    },
    size_min: {
        ...SIZE_FIELD,
        label: "Min size",
        badge: value => "At least " + fmt_size( value, false )
    },
    size_max: {
        ...SIZE_FIELD,
        label: "Max size",
        badge: value => "At most " + fmt_size( value, false )
    },
    backtrace_depth_min: {
        ...POSITIVE_INTEGER_FIELD,
        label: "Min backtrace depth",
        badge: value => "Backtrace at least " + value + " deep"
    },
    backtrace_depth_max: {
        ...POSITIVE_INTEGER_FIELD,
        label: "Max backtrace depth",
        badge: value => "Backtrace at most " + value + " deep"
    },
    function_regex: {
        ...REGEX_FIELD,
        label: "Function regex",
        badge: value => "Functions matching /" + value + "/"
    },
    negative_function_regex: {
        ...REGEX_FIELD,
        label: "Negative function regex",
        badge: value => "Functions NOT matching /" + value + "/"
    },
    source_regex: {
        ...REGEX_FIELD,
        label: "Source file regex",
        badge: value => "Sources matching /" + value + "/"
    },
    negative_source_regex: {
        ...REGEX_FIELD,
        label: "Negative source file regex",
        badge: value => "Sources NOT matching /" + value + "/"
    },
    backtraces: {
        label: "Backtrace",
        badge: value => "Matching backtrace with ID " + value
    },
    group_interval_min: {
        ...DURATION_OR_PERCENTAGE_FIELD,
        label: "Min group interval",
        badge: value => "Group interval at least " + fmt_or_percent( fmt_duration_for_display )( value )
    },
    group_interval_max: {
        ...DURATION_OR_PERCENTAGE_FIELD,
        label: "Max group interval",
        badge: value => "Group interval at most " + fmt_or_percent( fmt_duration_for_display )( value )
    },
    group_allocations_min: {
        ...POSITIVE_INTEGER_FIELD,
        label: "Min allocations",
        badge: value => "At least " + value + " allocations"
    },
    group_allocations_max: {
        ...POSITIVE_INTEGER_FIELD,
        label: "Max allocations",
        badge: value => "At most " + value + " allocations"
    },
    group_leaked_allocations_min: {
        ...POSITIVE_INTEGER_OR_PERCENTAGE_FIELD,
        label: "Min leaked allocations",
        badge: value => "At least " + value + " leaked allocations"
    },
    group_leaked_allocations_max: {
        ...POSITIVE_INTEGER_OR_PERCENTAGE_FIELD,
        label: "Max leaked allocations",
        badge: value => "At most " + value + " leaked allocations"
    },
    lifetime: {
        ...RADIO_FIELD,
        variants: {
            "": "Show all",
            only_leaked: "Only leaked",
            only_not_deallocated_in_current_range: "Only not deallocated in current time range",
            only_deallocated_in_current_range: "Only deallocated in current time range",
            only_temporary: "Only temporary",
            only_whole_group_leaked: "Only whole group leaked"
        },
        badge: {
            only_leaked: "Only leaked",
            only_not_deallocated_in_current_range: "Only not deallocated in current range",
            only_deallocated_in_current_range: "Only deallocated in current range",
            only_temporary: "Only temporary",
            only_whole_group_leaked: "Only whole group leaked"
        }
    },
    arena: {
        ...RADIO_FIELD,
        variants: {
            "": "Show all",
            main: "Only from main arena",
            non_main: "Only from non-main arena"
        },
        badge: {
            main: "Only from main arena",
            non_main: "Only from non-main arena"
        }
    },
    mmaped: {
        ...RADIO_FIELD,
        variants: {
            "": "Show all",
            yes: "Only mmaped",
            no: "Only non-mmaped"
        },
        badge: {
            yes: "Only mmaped",
            no: "Only non-mmaped"
        }
    }
};

function state_to_filter( fields, state ) {
    let output = {};
    if( !state ) {
        return output;
    }

    _.each( state, (raw_value, key) => {
        const field = fields[ key ];
        if( field.validate( raw_value ) ) {
            const value = field.parse( raw_value );
            output[ key ] = value;
        } else {
            output[ key ] = null;
        }
    });

    return output;
}

class FilterEditor extends React.Component {
    state = {}

    field( key ) {
        const kind = FIELDS[ key ].kind;
        const field = this.getFieldState( key );
        if( kind === "entry" ) {
            return (
                <div>
                    <Label for={key}>{FIELDS[ key ].label}</Label>
                    <Input
                        id={key}
                        valid={field.is_changed}
                        invalid={field.is_invalid}
                        value={field.value}
                        onChange={e => this.onChanged( key, e.target.value )}
                    />
                </div>
            );
        } else if( kind === "radio" ) {
            const groups = _.map( FIELDS[ key ].variants, (label, variant_key) => {
                return (
                    <FormGroup key={variant_key} check>
                        <Label check>
                            <Input
                                type="radio"
                                name={variant_key}
                                checked={field.value === variant_key}
                                onChange={e => {
                                    const new_value = e.target.value;
                                    if( new_value === "on" ) {
                                        this.onChanged( key, variant_key );
                                    }
                                }}
                            />{" "}
                            {label}
                        </Label>
                    </FormGroup>
                );
            });

            return (
                <FormGroup tag="div">
                    {groups}
                </FormGroup>
            );
        }
    }

    render() {
        return (
            <Tabbed>
                <div title="By timestamp" className="d-flex">
                    {this.field("from")}
                    <div className="px-2" />
                    {this.field("to")}
                </div>
                <div title="By size" className="d-flex">
                    {this.field("size_min")}
                    <div className="px-2" />
                    {this.field("size_max")}
                </div>
                <div title="By lifetime" className="d-flex">
                    {this.field("lifetime_min")}
                    <div className="px-2" />
                    {this.field("lifetime_max")}
                    <div className="px-2" />
                    {this.field("lifetime")}
                </div>
                <div title="By backtrace" className="d-flex flex-column">
                    <div className="d-flex flex-row">
                        {this.field("function_regex")}
                        <div className="px-2" />
                        {this.field("negative_function_regex")}
                        <div className="px-2" />
                        {this.field("source_regex")}
                        <div className="px-2" />
                        {this.field("negative_source_regex")}
                    </div>
                    <div className="d-flex flex-row">
                        {this.field("backtrace_depth_min")}
                        <div className="px-2" />
                        {this.field("backtrace_depth_max")}
                    </div>
                </div>
                <div title="By group (global)" className="d-flex flex-column">
                    <div className="d-flex flex-row">
                        {this.field("group_allocations_min")}
                        <div className="px-2" />
                        {this.field("group_allocations_max")}
                        <div className="px-2" />
                        {this.field("group_leaked_allocations_min")}
                        <div className="px-2" />
                        {this.field("group_leaked_allocations_max")}
                    </div>
                    <div className="d-flex flex-row">
                        {this.field("group_interval_min")}
                        <div className="px-2" />
                        {this.field("group_interval_max")}
                    </div>

                </div>
                <div title="Misc" className="d-flex">
                    {this.field("mmaped")}
                    <div className="px-2" />
                    {this.field("arena")}
                </div>
            </Tabbed>
        );
    }

    getFieldState( key ) {
        let original = this.props.filter[ key ];
        if( original === undefined || original === null ) {
            original = "";
        } else {
            original = FIELDS[ key ].format( original );
        }

        let value = (this.props.state || {})[ key ];
        if( value === undefined || value === null ) {
            value = original;
        }

        const is_invalid = value !== "" && !FIELDS[ key ].validate( value );
        const is_changed = !is_invalid && value !== original;
        return {
            value,
            original,
            is_invalid,
            is_changed
        };
    }

    onChanged( key, value ) {
        value = value.trim().replace( / {2,}/, " " ).replace( /\s*:\s*/, ":" ).replace( /\s*-\s*/, "-" );
        const new_state = {...this.props.state};
        new_state[ key ] = value;

        if( this.getFieldState( key ).original === value ) {
            delete new_state[ key ];
        }

        this.props.onChanged( new_state );
    }
}

class Control extends React.Component {
    state = {}

    constructor() {
        super()
        this.toggleFilterEdit = this.toggleFilterEdit.bind( this );
    }

    static getDerivedStateFromProps( props, state ) {
        state.editedFilter = props.filter;
        return state;
    }

    render() {
        const badges = [];
        if( !_.isNil( this.props.totalCount ) ) {
            badges.push(
                <Badge key={0} className="filter-pill" color="primary" pill>Total: {this.props.totalCount}</Badge>
            );
        }

        const add = (value) => {
            badges.push(
                <Badge key={badges.length} className="filter-pill" color="info" pill>{value}</Badge>
            );
        };

        const filter = this.props.filter;
        _.each( FIELDS, (field, key) => {
            const value = filter[ key ];
            if( _.isNil( value ) ) {
                return;
            }

            if( _.isFunction( field.badge ) ) {
                add( field.badge( value ) );
            } else {
                add( field.badge[ value ] );
            }
        });

        let fullDataUrl;
        let heaptrackUrl;
        let treeUrl;
        let flamegraphUrl;
        if( this.props.dataUrl ) {
            const data_url = new URL( this.props.dataUrl );
            const q = _.omit( extract_query( data_url.search ), "count", "skip" );
            data_url.search = "?" + create_query( q ).toString();
            fullDataUrl = data_url.toString();

            data_url.pathname = "/data/" + this.props.id + "/export/heaptrack/heaptrack.dat";
            heaptrackUrl = data_url.toString();

            data_url.pathname = "/data/" + this.props.id + "/allocation_ascii_tree";
            treeUrl = data_url.toString();

            data_url.pathname = "/data/" + this.props.id + "/export/flamegraph/flame.svg";
            flamegraphUrl = data_url.toString();
        }

        return (
            <div className="navbar flex-column flex-md-nonwrap p-0 shadow w-100 px-3">
                <div className="d-flex justify-content-between w-100">
                    <div className="d-flex align-items-center">
                        <Link to="/" className="mr-3"><Feather name="grid" /></Link>
                        <Link to={"/overview/" + this.props.id} className="mr-3"><Feather name="bar-chart-2" /></Link>
                        <Link to={this.props.location} className="mr-3"><Feather name="anchor" /></Link>
                        <div>
                            {badges}
                        </div>
                    </div>
                    <div className="d-flex align-items-baseline py-1">
                        <Label check>
                            <Input type="checkbox" id="show-full-backtraces" checked={this.props.showFullBacktraces} onChange={this.onShowFullBacktracesChanged.bind(this)} />{' '}
                            Full backtraces
                        </Label>
                        <Label check className="ml-4">
                            <Input type="checkbox" id="group-by-backtraces" checked={this.props.groupByBacktraces} onChange={this.onGroupByBacktracesChanged.bind(this)} />{' '}
                            Group by backtraces
                        </Label>
                        <Label className="ml-2">Page</Label>
                        <Input className="ml-2" type="number" name="number" id="exampleNumber" placeholder="" style={{width: "6em"}} value={this.props.page + 1} onChange={e => this.onPageChange( e.target.value - 1 )} />
                        <Button outline color="primary" className="ml-2 btn-sm" onClick={this.toggleFilterEdit} style={{minWidth: "8em"}}>{
                            ( !this.state.editingFilter ) ? ("Change filter...") : ("Apply filter")
                        }</Button>
                        <div className="ml-3">
                            <ContextMenuTrigger id="allocations_navbar_context_menu" ref={c => this.menu_trigger = c}>
                                <Feather name="menu" onClick={event => this.menu_trigger.handleContextClick( event )} />
                            </ContextMenuTrigger>
                            <ContextMenu id="allocations_navbar_context_menu">
                                <MenuItem>
                                    <a href={flamegraphUrl || "#"}>Open flamegraph</a>
                                </MenuItem>
                                <MenuItem>
                                    <a href={fullDataUrl || "#"}>Download as JSON (every page)</a>
                                </MenuItem>
                                <MenuItem>
                                    <a href={this.props.dataUrl || "#"}>Download as JSON (only this page)</a>
                                </MenuItem>
                                <MenuItem>
                                    <a href={heaptrackUrl || "#"}>Download as Heaptrack data file</a>
                                </MenuItem>
                                <MenuItem>
                                    <a href={treeUrl || "#"}>Download as ASCII tree</a>
                                </MenuItem>
                            </ContextMenu>
                        </div>
                    </div>
                </div>
                <div className={classNames( "pb-2 w-100 justify-content-between", {"d-none": !this.state.editingFilter, "d-flex": this.state.editingFilter} )}>
                    <FilterEditor filter={this.props.filter} state={this.state.filterEditorState} onChanged={this.onFilterChanged.bind( this )} />
                </div>
            </div>
        );
    }

    toggleFilterEdit() {
        let filter_update = null;
        if( this.state.editingFilter ) {
            filter_update = state_to_filter( FIELDS, this.state.filterEditorState );
        }

        this.setState({
            filterEditorState: null,
            editingFilter: !this.state.editingFilter
        });

        if( filter_update && !_.isEmpty( filter_update ) && this.props.onFilterChange ) {
            this.props.onFilterChange( {...this.props.filter, ...filter_update} );
        }

    }

    onFilterChanged( filterEditorState ) {
        this.setState( {filterEditorState} );
    }

    onPageChange( page ) {
        if( this.props.onPageChange ) {
            this.props.onPageChange( page );
        }
    }

    onShowFullBacktracesChanged( event ) {
        if( this.props.onShowFullBacktracesChanged ) {
            this.props.onShowFullBacktracesChanged( event.target.checked )
        }
    }

    onGroupByBacktracesChanged( event ) {
        if( this.props.onGroupByBacktracesChanged ) {
            this.props.onGroupByBacktracesChanged( event.target.checked )
        }
    }
}

class Filter {
    constructor( location ) {
        const q = new URLSearchParams( location.search );
        this.from = q.get( "from" );
        this.to = q.get( "to" );
        this.size_min = q.get( "size_min" );
        this.size_max = q.get( "size_max" );
    }
}

function get_data_url( source_url, id, params ) {
    params = {...params};
    if( !_.isNil( params.page_size ) ) {
        params.count = params.page_size;
        params.skip = params.page_size * params.page;
    }

    const group_allocations = params.group_allocations === "true" || params.group_allocations === "1";
    let source;
    if( group_allocations ) {
        source = "allocation_groups";
    } else {
        source = "allocations";
    }

    const encoded_body = create_query( _.omit( params, "page", "page_size", "show_full_backtraces", "group_allocations" ) ).toString();
    const data_url = source_url + "/data/" + id + "/" + source + "?" + encoded_body;
    return data_url;
}

function timestamp_cell( absolute, relative, relative_p ) {
    const percent = parseInt( relative_p * 100, 10 );
    return <div className="d-flex flex-row flex-column align-items-end"><div>{fmt_date_timeval( absolute )}</div><div>{fmt_uptime_timeval( relative )}, {percent}%</div><div></div></div>;
}

function backtrace_cell( show_full_backtraces, frames ) {
    let last_frame_index = frames.length - 1;

    while( frames[ last_frame_index - 1 ] !== undefined ) {
        const frame = frames[ last_frame_index ];
        if( frame.function ) {
            if( frame.function.match( /^(__gnu_cxx::|std::allocator_traits<|std::_Vector_base|std::_Deque_base|std::__allocated_ptr|std::__shared_count|std::__shared_ptr|boost::detail::shared_count|void boost::detail::sp_pointer_construct|std::string::_M_create|std::string::_M_mutate|std::string::_M_replace_aux|std::_Function_base)/ ) ) {
                last_frame_index -= 1;
                continue;
            }

            const m = frame.function.match( /::_M_([a-zA-Z0-9_]+)/ );
            if( m ) {
                const name = m[ 1 ].replace( /_aux$/, "" );
                if( frames[ last_frame_index - 1 ] && (frames[ last_frame_index - 1 ].function || "").includes( name ) ) {
                    last_frame_index -= 1;
                    continue;
                }
            }
        }

        break;
    }

    let indices = [last_frame_index, last_frame_index - 1];
    frames = frames.map((frame) => {
        let weight = 1.0;
        let shift = 0;
        if( frame.function ) {
            let d = frame.function.length - 120;
            if( d < 0 ) {
                d = 0;
            }
            weight += 0.3 * (d / 100);
            if( frame.function.match( /^(std|boost|void std)::/ ) ) {
                shift += 10;
            }
        }
        return {...frame, count: (frame.count * weight + shift) | 0 };
    });

    const minimums = {}
    for( let i = 0; i < last_frame_index - 2; ++i ) {
        const frame = frames[ i ];
        if( minimums[ frame.library ] === undefined || minimums[ frame.library ] > frame.count ) {
            minimums[ frame.library ] = frame.count;
        }
    }

    const min_count = _.min( _.map( frames, "count" ).slice( 0, last_frame_index - 2 ) );
    if( show_full_backtraces ) {
        for( let i = 0; i < frames.length; ++i ) {
            indices.push( i );
        }
    } else {
        for( let i = 0; i < last_frame_index - 2; ++i ) {
            const frame = frames[ i ];
            if( frame.count === minimums[ frame.library ] ) {
                indices.push( i );
            }
            if( frame.count === min_count ) {
                indices.push( i - 1 );
                indices.push( i - 2 );
            }
        }
    }

    indices.push( 0 );
    indices.sort((a, b) => a - b);
    indices = _.uniq( indices ).filter( index => index >= 0 && index < frames.length );

    let out = [];
    for( let i = 0; i < indices.length; ++i ) {
        let index = indices[ i ];
        const frame = frames[ index ];
        const entry = format_frame( index, frame );
        out.push( entry );
    }

    return out;
}

export default class PageDataAllocations extends React.Component {
    state = { pages: null, data: {}, loading: false };

    render() {
        const q = new URLSearchParams( this.props.location.search );
        const page = (parseInt( q.get( "page" ), 10 ) || 1) - 1;
        const page_size = parseInt( q.get( "page_size" ), 10 ) || 20;
        const show_full_backtraces = q.get( "show_full_backtraces" ) === "true" || q.get( "show_full_backtraces" ) === "1";
        const group_by_backtraces = q.get( "group_allocations" ) === "true" || q.get( "group_allocations" ) === "1";

        const columns = [
            {
                id: "timestamp",
                Header: "Timestamp",
                Cell: cell => {
                    return timestamp_cell( cell.original.timestamp, cell.original.timestamp_relative, cell.original.timestamp_relative_p );
                },
                maxWidth: 160,
                view: "allocations"
            },
            {
                id: "lifetime",
                Header: "Lifetime",
                accessor: entry => {
                    if( entry.deallocation === null ) {
                        return "∞";
                    } else {
                        let interval = {
                            secs: entry.deallocation.timestamp.secs - entry.timestamp.secs,
                            fract_nsecs: entry.deallocation.timestamp.fract_nsecs - entry.timestamp.fract_nsecs,
                        };

                        if( interval.fract_nsecs < 0 ) {
                            interval.secs -= 1;
                            interval.fract_nsecs += 1000000000;
                        }

                        return fmt_uptime_timeval( interval );
                    }
                },
                maxWidth: 60,
                sortable: false,
                view: "allocations"
            },
            {
                Header: "Address",
                accessor: "address_s",
                maxWidth: 140,
                view: "allocations"
            },
            {
                Header: "Thread",
                Cell: cell => {
                    return fmt_hex16( cell.value );
                },
                accessor: "thread",
                maxWidth: 60,
                sortable: false,
                view: "allocations"
            },
            {
                Header: "Size",
                Cell: cell => {
                    return <div>{fmt_size( cell.value )}<br />({fmt_size( cell.value + cell.original.extra_space )})</div>;
                },
                accessor: "size",
                maxWidth: 85,
                view: "allocations"
            },
            {
                Header: "Mmaped",
                Cell: cell => {
                    if( cell.value ) {
                        return "Yes";
                    } else {
                        return "No";
                    }
                },
                accessor: "is_mmaped",
                maxWidth: 70,
                sortable: false,
                view: "allocations"
            },
            {
                Header: "Arena",
                Cell: cell => {
                    if( cell.value ) {
                        return "Main";
                    } else {
                        return "Other";
                    }
                },
                accessor: "in_main_arena",
                maxWidth: 70,
                sortable: false,
                view: "allocations"
            },

            {
                id: "all.min_timestamp",
                Header: <div>(global)<br />First allocation</div>,
                Cell: cell => {
                    return timestamp_cell( cell.original.all.min_timestamp, cell.original.all.min_timestamp_relative, cell.original.all.min_timestamp_relative_p );
                },
                maxWidth: 160,
                view: "grouped"
            },
            {
                id: "all.max_timestamp",
                Header: <div>(global)<br />Last allocation</div>,
                Cell: cell => {
                    return timestamp_cell( cell.original.all.max_timestamp, cell.original.all.max_timestamp_relative, cell.original.all.max_timestamp_relative_p );
                },
                maxWidth: 160,
                view: "grouped"
            },
            {
                id: "all.interval",
                Header: <div>(global)<br />Interval</div>,
                Cell: cell => {
                    return fmt_uptime_timeval( cell.original.all.interval );
                },
                maxWidth: 95,
                view: "grouped"
            },
            {
                id: "all.allocated_count",
                Header: <div>(global)<br />Allocated</div>,
                Cell: cell => {
                    return cell.original.all.allocated_count;
                },
                maxWidth: 70,
                view: "grouped"
            },
            {
                id: "all.leaked_count",
                Header: <div>(global)<br />Leaked</div>,
                Cell: cell => {
                    return cell.original.all.leaked_count;
                },
                maxWidth: 75,
                view: "grouped"
            },

            {
                id: "only_matched.min_timestamp",
                Header: <div>(matched)<br />First allocation</div>,
                Cell: cell => {
                    return timestamp_cell( cell.original.only_matched.min_timestamp, cell.original.only_matched.min_timestamp_relative, cell.original.only_matched.min_timestamp_relative_p );
                },
                maxWidth: 160,
                view: "grouped"
            },
            {
                id: "only_matched.max_timestamp",
                Header: <div>(matched)<br />Last allocation</div>,
                Cell: cell => {
                    return timestamp_cell( cell.original.only_matched.max_timestamp, cell.original.only_matched.max_timestamp_relative, cell.original.only_matched.max_timestamp_relative_p );
                },
                maxWidth: 160,
                view: "grouped"
            },
            {
                id: "only_matched.interval",
                Header: <div>(matched)<br />Interval</div>,
                Cell: cell => {
                    return fmt_uptime_timeval( cell.original.only_matched.interval );
                },
                maxWidth: 95,
                view: "grouped"
            },
            {
                id: "only_matched.allocated_count",
                Header: <div>(matched)<br />Allocated</div>,
                Cell: cell => {
                    return cell.original.only_matched.allocated_count;
                },
                maxWidth: 75,
                view: "grouped"
            },
            {
                id: "only_matched.leaked_count",
                Header: <div>(matched)<br />Leaked</div>,
                Cell: cell => {
                    return cell.original.only_matched.leaked_count;
                },
                maxWidth: 75,
                view: "grouped"
            },
            {
                id: "only_matched.size",
                Header: <div>(matched)<br />Size</div>,
                Cell: cell => {
                    return fmt_size( cell.original.only_matched.size );
                },
                maxWidth: 85,
                view: "grouped"
            }
        ].filter( (column) => {
            if( column.view === "allocations" && this.state.group ) {
                return false;
            }

            if( column.view === "grouped" && !this.state.group ) {
                return false;
            }

            return true;
        });

        const get_sorted = (sorted) => {
            let sort_by = null;
            let order = null;
            if( sorted.length > 0 ) {
                sort_by = sorted[ 0 ].id;
                order = sorted[ 0 ].desc ? "dsc" : "asc";
            }

            return { sort_by, order };
        };

        const table_sorted = [];
        const p = extract_query( this.props.location.search );
        if( p.sort_by || p.order ) {
            table_sorted[ 0 ] = {
                id: p.sort_by || "timestamp",
                desc: (p.order || "asc") === "dsc"
            };
        }

        let expanded = {};
        for( let i = 0; i < page_size; ++i ) {
            expanded[ i ] = true;
        }

        let placeholder = {"xs":[1594837555, 	1594837555], "allocated_size":[1, 2]};

        return (
            <div className="PageDataAllocations">
                <Control
                    id={this.props.id}
                    location={this.props.location}
                    totalCount={this.state.data.total_count}
                    page={page}
                    pageSize={page_size}
                    showFullBacktraces={show_full_backtraces}
                    groupByBacktraces={group_by_backtraces}
                    filter={extract_query( this.props.location.search )}
                    dataUrl={this.state.lastDataUrl}
                    onPageChange={(page) => update_query( this.props, {page: page + 1} )}
                    onShowFullBacktracesChanged={value => {
                        update_query( this.props, {show_full_backtraces: value} );
                    }}
                    onGroupByBacktracesChanged={value => {
                        const location = update_query( this.props, {
                            group_allocations: value,
                            sort_by: this.state.otherSortBy,
                            order: this.state.otherOrder
                        });
                        this.setState({
                            otherSortBy: p.sort_by,
                            otherOrder: p.order
                        });
                        const params = extract_query( location.search );
                        this.fetchData( {...params, page, page_size} );
                    }}
                    onFilterChange={(filter) => {
                        update_query( this.props, filter );
                        this.fetchData( {...filter, page, page_size} );
                    }}
                />
                <ReactTable
                    manual
                    data={this.state.data.allocations}
                    pages={this.state.pages}
                    loading={this.state.loading}
                    columns={columns}
                    defaultSorted={table_sorted}
                    page={page}
                    pageSize={page_size}
                    onFetchData={(state) => {
                        const params = extract_query( this.props.location.search );
                        params.page = state.page;
                        params.page_size = state.pageSize;
                        this.fetchData( {...params, ...get_sorted( state.sorted )} );
                    }}
                    onSortedChange={(sorted) => {
                        const s = get_sorted( sorted );
                        update_query( this.props, s );
                    }}
                    onPageSizeChange={(page_size, page) => update_query( this.props, {page: page + 1, page_size} )}
                    showPaginationBottom={false}
                    SubComponent={row => {
                        const cell = backtrace_cell( show_full_backtraces, row.original.backtrace );

                        const params = extract_query( this.props.location.search );
                        const q = _.omit( params, "count", "skip", "group_allocations", "sort_by", "order" );
                        q.backtraces = row.original.backtrace_id;
                        const url = "/#" + this.props.location.pathname + "?" + create_query( q ).toString();

                        const group_allocations = params.group_allocations === "true" || params.group_allocations === "1";
                        if(group_allocations) {
                            return <div className="backtrace-cell" onContextMenu={event => {
                                this.setState({
                                    showOnlyAllocationsUrl: url
                                });
                                return this.menu_trigger.handleContextClick( event );
                            }}>  
                            <BacktraceGraph
                                key={String(this.props.id)+"_"+String(row.original.backtrace_id)+String(Math.random())}
                                title="Memory usage"
                                data={placeholder}
                                y_accessor="allocated_size"
                                y_label=""
                                x0={this.state.x0}
                                x1={this.state.x1}
                                fill={true}
                                xUnit="unix_timestamp"
                                id={this.props.id}
                                sourceUrl={this.props.sourceUrl}
                                backtrace_id={row.original.backtrace_id}
                            />
                            {cell}</div>;
                        } else {
                            return <div className="backtrace-cell" onContextMenu={event => {
                                this.setState({
                                    showOnlyAllocationsUrl: url
                                });
                                return this.menu_trigger.handleContextClick( event );
                            }}>{cell}</div>;
                        }
                    }}
                    expanded={expanded}
                />
                <ContextMenuTrigger id="allocation_context_menu" ref={c => this.menu_trigger = c}>
                    <div />
                </ContextMenuTrigger>
                <ContextMenu id="allocation_context_menu">
                    <MenuItem>
                        <a href={this.state.showOnlyAllocationsUrl || "#"}>Show only allocations with this backtrace...</a>
                    </MenuItem>
                </ContextMenu>
            </div>
        );
    }

    fetchData( params ) {
        if( this.state.loading ) {
            return;
        }

        const data_url = get_data_url( this.props.sourceUrl, this.props.id, params );
        if( this.state.lastDataUrl === data_url ) {
            return;
        }

        this.setState( { loading: true } );
        fetch( data_url )
            .then( response => response.json() )
            .then( data => {
                const pages = Math.floor( (data.total_count / params.page_size) ) + (((data.total_count % params.page_size) !== 0) ? 1 : 0);
                this.setState({
                    data,
                    pages,
                    loading: false,
                    lastDataUrl: data_url,
                    group: params.group_allocations === "true" || params.group_allocations === "1"
                });
            });
    }
}
